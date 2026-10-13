use crate::settings::{Settings, Snapshot, Store};
use crate::{
    core::{self, MessageKey, QuerySpec, ScanResult},
    gateway::{Result, TelegramGateway},
    services::{ActionService, AuthService, Chat, ChatService, ConnectConfig, ForwardReport},
    tdlib::gateway::TdlibGateway,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Serialize)]
struct Connection {
    profile_id: Option<String>,
    generation: u64,
    auth_state: String,
    connection_state: String,
    detail: String,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            profile_id: None,
            generation: 0,
            auth_state: "Disconnected".into(),
            connection_state: "Disconnected".into(),
            detail: "请输入配置并连接".into(),
        }
    }
}
#[derive(Default)]
struct AppState {
    lifecycle: Mutex<()>,
    store: Mutex<Option<Store>>,
    profile: Mutex<Option<String>>,
    epoch: AtomicU64,
    pending: Mutex<HashMap<String, String>>,
    gateway: Mutex<Option<Arc<TdlibGateway>>>,
    connecting: AtomicBool,
    closing: AtomicBool,
    connection: Mutex<Connection>,
    active: Mutex<Option<(String, Arc<AtomicBool>)>>,
    results: Mutex<Option<ScanResult>>,
    forwarding: AtomicBool,
    sent_updates: Mutex<HashMap<String, String>>,
}
fn gateway(s: &AppState) -> Result<Arc<TdlibGateway>> {
    if s.closing.load(Ordering::SeqCst) || s.connecting.load(Ordering::SeqCst) {
        return Err("Session transition in progress".into());
    }
    s.gateway
        .lock()
        .unwrap()
        .clone()
        .ok_or("Connect to Telegram first".into())
}
fn selection(s: &AppState, run_id: &str, post_keys: &[String]) -> Result<Vec<MessageKey>> {
    let results = s.results.lock().unwrap();
    let r = results.as_ref().ok_or("Run a query first")?;
    if r.run_id != run_id || r.status == core::ScanStatus::Scanning {
        return Err("Query selection is stale or still scanning".into());
    }
    let mut keys = Vec::new();
    for key in post_keys {
        let p = r
            .posts
            .iter()
            .find(|p| &p.key == key)
            .ok_or("Post is no longer in results")?;
        keys.extend(p.members.iter().map(|m| m.key.clone()));
    }
    Ok(keys)
}
fn retain_result(s: &AppState, result: &ScanResult, epoch: u64) -> bool {
    let active = s.active.lock().unwrap();
    if !current_epoch(s, epoch) || active.as_ref().map(|(id, _)| id) != Some(&result.run_id) {
        return false;
    }
    *s.results.lock().unwrap() = Some(result.clone());
    true
}
async fn background<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "Background task failed".to_string())?
}
fn update(app: &tauri::AppHandle, epoch: u64, v: Value) {
    let s = app.state::<AppState>();
    if !current_epoch(&s, epoch) {
        return;
    }
    if v["@type"] == "updateAuthorizationState" || v["@type"] == "updateConnectionState" {
        let mut connection = s.connection.lock().unwrap();
        connection.generation = epoch;
        connection.profile_id = s.profile.lock().unwrap().clone();
        if v["@type"] == "updateAuthorizationState" {
            connection.auth_state = v["authorization_state"]["@type"]
                .as_str()
                .unwrap_or("Unknown")
                .into();
            connection.detail = match connection.auth_state.as_str() {
                "authorizationStateReady" => "已登录；Session 可在下次启动复用",
                "authorizationStateWaitTdlibParameters" => "等待 API 配置和数据库密码",
                "authorizationStateWaitPhoneNumber" => "请输入带国家代码的手机号",
                "authorizationStateWaitCode" => "请输入 Telegram 登录验证码",
                "authorizationStateWaitPassword" => "请输入 Telegram 两步验证密码",
                "authorizationStateWaitEmailAddress" => "请输入授权邮箱",
                "authorizationStateWaitEmailCode" => "请输入邮箱验证码",
                "authorizationStateClosing" => "正在安全关闭数据库",
                "authorizationStateClosed" => "已安全关闭",
                _ => "当前授权步骤不受支持；请用官方 Telegram 客户端完成后重试",
            }
            .into();
        } else {
            connection.connection_state = v["state"]["@type"].as_str().unwrap_or("Unknown").into();
        }
        let ready = connection.auth_state == "authorizationStateReady"
            && v["@type"] == "updateAuthorizationState";
        let _ = app.emit("connection", connection.clone());
        drop(connection);
        if ready {
            let handle = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let s = handle.state::<AppState>();
                let id = s.profile.lock().unwrap().clone();
                let g = s.gateway.lock().unwrap().clone();
                if let Some(g) = g {
                    let me = g.request(json!({"@type":"getMe"}), &AtomicBool::new(false));
                    if !current_epoch(&s, epoch) {
                        return;
                    }
                    if let Ok(me) = me
                        && let Some(id) = id
                    {
                        let outcome = with_store(&s, |store| {
                            if !current_epoch(&s, epoch) {
                                return Ok(false);
                            }
                            store.identify(&id, &me)
                        });
                        match outcome {
                            Ok(duplicate) => {
                                let _ = handle.emit("accounts-changed", ());
                                if duplicate {
                                    let _=handle.emit("account-notice","此 Telegram 用户已有另一 Profile；目录仍独立，请保留一个并移除多余 Profile。");
                                }
                            }
                            Err(_) => {
                                let _ = handle.emit(
                                    "account-notice",
                                    "账号身份已读取但保存失败；请检查配置目录。",
                                );
                            }
                        }
                    }
                }
            });
        }
    }
    if v["@type"] == "updateMessageSendSucceeded" || v["@type"] == "updateMessageSendFailed" {
        let target = v["message"]["chat_id"].as_i64().unwrap_or(0).to_string();
        let old = v["old_message_id"].as_i64().unwrap_or(0).to_string();
        let state = if v["@type"] == "updateMessageSendSucceeded" {
            "Sent"
        } else {
            "Failed"
        };
        let mut updates = s.sent_updates.lock().unwrap();
        updates.insert(format!("{target}:{old}"), state.into());
        s.pending.lock().unwrap().remove(&format!("{target}:{old}"));
        drop(updates);
        let _ = app.emit(
            "forward-update",
            json!({"target":target,"old_message_id":old,"state":state,"generation":epoch}),
        );
    }
}
#[tauri::command]
fn connection_status(s: State<AppState>) -> Connection {
    s.connection.lock().unwrap().clone()
}
fn with_store<T>(s: &AppState, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
    let mut guard = s.store.lock().unwrap();
    if guard.is_none() {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .ok_or("LOCALAPPDATA unavailable")?
            .join("com.funare.tmsgpicker");
        *guard = Some(Store::open(root)?);
    }
    f(guard.as_mut().unwrap())
}
#[tauri::command]
fn app_settings(s: State<AppState>) -> Result<Snapshot> {
    with_store(&s, |store| Ok(store.snapshot()))
}
#[derive(Deserialize)]
struct SettingsEdit {
    settings: Settings,
    api_hash: Option<String>,
    proxy_password: Option<String>,
}
#[tauri::command]
fn save_settings(s: State<AppState>, edit: SettingsEdit) -> Result<Snapshot> {
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Session operation in progress")?;
    if s.gateway.lock().unwrap().is_some() {
        return Err("安全断开后才能修改 API 或代理设置".into());
    }
    with_store(&s, |store| {
        store.set_settings(edit.settings)?;
        if let Some(v) = edit.api_hash {
            store.set_secret("api", Some(v))?;
        }
        if let Some(v) = edit.proxy_password {
            store.set_secret("proxy", Some(v))?;
        }
        Ok(store.snapshot())
    })
}
#[tauri::command]
fn save_layout(s: State<AppState>, left: f64, right: f64) -> Result<()> {
    with_store(&s, |store| {
        let mut settings = store.settings();
        settings.left_ratio = left;
        settings.right_ratio = right;
        store.set_settings(settings)
    })
}
#[tauri::command]
fn reset_settings(s: State<AppState>) -> Result<Snapshot> {
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Session operation in progress")?;
    if s.gateway.lock().unwrap().is_some() {
        return Err("Disconnect before resetting settings".into());
    }
    with_store(&s, |store| {
        store.reset()?;
        Ok(store.snapshot())
    })
}
#[tauri::command]
fn clear_secret(s: State<AppState>, key: String) -> Result<Snapshot> {
    with_store(&s, |store| {
        store.set_secret(&key, None)?;
        if key == "proxy" {
            let mut settings = store.settings();
            settings.proxy_username.clear();
            store.set_settings(settings)?;
        }
        Ok(store.snapshot())
    })
}
#[tauri::command]
fn add_account(s: State<AppState>, name: String) -> Result<String> {
    with_store(&s, |store| store.add(name))
}
#[tauri::command]
fn rename_account(s: State<AppState>, profile_id: String, name: String) -> Result<Snapshot> {
    with_store(&s, |store| {
        store.rename(&profile_id, name)?;
        Ok(store.snapshot())
    })
}
#[tauri::command]
fn remove_account(
    s: State<AppState>,
    profile_id: String,
    confirmed: bool,
    delete_data: bool,
    delete_confirmed: bool,
) -> Result<Snapshot> {
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Session operation in progress")?;
    if !confirmed {
        return Err("Confirm local profile removal".into());
    }
    if s.profile.lock().unwrap().as_ref() == Some(&profile_id)
        && s.gateway.lock().unwrap().is_some()
    {
        return Err("Disconnect this account first".into());
    }
    // Session deletion is opt-in and separately confirmed; legacy deletion is refused.
    with_store(&s, |store| {
        if delete_data {
            if !delete_confirmed {
                return Err("Second confirmation required for Session deletion".into());
            }
            store.delete_session(&profile_id)?;
        }
        store.remove(&profile_id)?;
        Ok(store.snapshot())
    })
}
fn clear_session(s: &AppState) {
    if let Some((_, cancel)) = s.active.lock().unwrap().take() {
        cancel.store(true, Ordering::Relaxed);
    }
    *s.results.lock().unwrap() = None;
    s.sent_updates.lock().unwrap().clear();
}
fn can_transition(s: &AppState) -> Result<()> {
    if s.forwarding.load(Ordering::SeqCst) || !s.pending.lock().unwrap().is_empty() {
        return Err("仍有正在提交或 Pending 的转发；等待最终发送状态后再切换或断开".into());
    }
    Ok(())
}
fn current_epoch(s: &AppState, epoch: u64) -> bool {
    s.epoch.load(Ordering::SeqCst) == epoch
}
fn finish_close(s: &AppState, outcome: Result<()>) -> Result<Connection> {
    outcome?;
    *s.gateway.lock().unwrap() = None;
    s.epoch.fetch_add(1, Ordering::SeqCst);
    let c = Connection {
        profile_id: s.profile.lock().unwrap().clone(),
        generation: s.epoch.load(Ordering::SeqCst),
        ..Connection::default()
    };
    *s.connection.lock().unwrap() = c.clone();
    s.closing.store(false, Ordering::SeqCst);
    Ok(c)
}
fn close_session(app: &tauri::AppHandle, logout: bool) -> Result<()> {
    let s = app.state::<AppState>();
    can_transition(&s)?;
    s.closing.store(true, Ordering::SeqCst);
    clear_session(&s);
    let outcome = (|| {
        let g = s.gateway.lock().unwrap().clone();
        if let Some(g) = g {
            if logout {
                g.request(json!({"@type":"logOut"}), &AtomicBool::new(false))?;
                g.wait_closed()?;
            } else {
                g.shutdown()?;
            }
        }
        Ok(())
    })();
    let c = finish_close(&s, outcome)?;
    let _ = app.emit("connection", c);
    Ok(())
}
#[derive(Deserialize)]
struct ActivateConfig {
    profile_id: String,
    passphrase: Option<String>,
    remember_database: bool,
}
#[tauri::command]
async fn connect(app: tauri::AppHandle, config: ActivateConfig) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        can_transition(&s)?;
        let (root, c) = with_store(&s, |store| {
            let root = store.session_root(&config.profile_id)?;
            let settings = store.settings();
            let key = format!("db:{}", config.profile_id);
            let passphrase = match config.passphrase.filter(|v| !v.is_empty()) {
                Some(v) => v,
                None => store
                    .secret(&key)?
                    .ok_or("请输入此账号现有数据库口令（新账号请选择口令）")?,
            };
            let api_hash = store.secret("api")?;
            let password = if settings.proxy_enabled {
                store.secret("proxy")?.unwrap_or_default()
            } else {
                String::new()
            };
            if config.remember_database {
                store.set_secret(&key, Some(passphrase.clone()))?;
            } else {
                store.set_secret(&key, None)?;
            }
            Ok((
                root,
                ConnectConfig {
                    api_id: settings.api_id,
                    api_hash,
                    passphrase,
                    proxy: if settings.proxy_enabled {
                        Some(crate::services::ProxyConfig {
                            server: settings.proxy_server,
                            port: settings.proxy_port,
                            username: settings.proxy_username,
                            password,
                        })
                    } else {
                        None
                    },
                },
            ))
        })?;
        close_session(&app, false)?;
        s.connecting.store(true, Ordering::SeqCst);
        let outcome = (|| {
            with_store(&s, |store| store.select(&config.profile_id))?;
            *s.profile.lock().unwrap() = Some(config.profile_id.clone());
            let epoch = s.epoch.fetch_add(1, Ordering::SeqCst) + 1;
            let handle = app.clone();
            let g = TdlibGateway::new(move |v| update(&handle, epoch, v))?;
            *s.gateway.lock().unwrap() = Some(g.clone());
            let state = g.request(
                json!({"@type":"getAuthorizationState"}),
                &AtomicBool::new(false),
            )?;
            update(
                &app,
                epoch,
                json!({"@type":"updateAuthorizationState","authorization_state":state}),
            );
            AuthService::initialize(g.as_ref(), c, &root)
        })();
        s.connecting.store(false, Ordering::SeqCst);
        outcome
    })
    .await
}
#[tauri::command]
async fn auth_submit(app: tauri::AppHandle, value: String) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        let state = s.connection.lock().unwrap().auth_state.clone();
        AuthService::submit(gateway(&s)?.as_ref(), &state, value)
    })
    .await
}
#[tauri::command]
async fn disconnect(app: tauri::AppHandle) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        close_session(&app, false)
    })
    .await
}
#[tauri::command]
async fn logout_account(app: tauri::AppHandle, profile_id: String, confirmed: bool) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        if !confirmed || s.profile.lock().unwrap().as_ref() != Some(&profile_id) {
            return Err("Confirm logout of the active account".into());
        }
        if s.connection.lock().unwrap().auth_state != "authorizationStateReady" {
            return Err("Account must be ready to log out".into());
        }
        close_session(&app, true)
    })
    .await
}
#[tauri::command]
async fn chats(app: tauri::AppHandle, search: String) -> Result<Vec<Chat>> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ChatService::list(gateway(&s)?.as_ref(), &search)
    })
    .await
}
#[tauri::command]
async fn public_chat(app: tauri::AppHandle, username: String) -> Result<Chat> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ChatService::public(gateway(&s)?.as_ref(), &username)
    })
    .await
}
#[tauri::command]
async fn saved_messages(app: tauri::AppHandle) -> Result<Chat> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ChatService::saved(gateway(&s)?.as_ref())
    })
    .await
}
#[tauri::command]
fn start_query(
    app: tauri::AppHandle,
    s: State<AppState>,
    spec: QuerySpec,
    run_id: String,
) -> Result<()> {
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Session operation in progress")?;
    spec.validate()?;
    if s.closing.load(Ordering::SeqCst) || s.connecting.load(Ordering::SeqCst) {
        return Err("Session transition in progress".into());
    }
    if run_id.is_empty() || run_id.len() > 100 {
        return Err("Invalid run ID".into());
    }
    let g = gateway(&s)?;
    let epoch = s.epoch.load(Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some((_, old)) = s
        .active
        .lock()
        .unwrap()
        .replace((run_id.clone(), cancel.clone()))
    {
        old.store(true, Ordering::Relaxed);
    }
    *s.results.lock().unwrap() = None;
    tauri::async_runtime::spawn_blocking(move || {
        let publish = |result: ScanResult| {
            let s = app.state::<AppState>();
            if retain_result(&s, &result, epoch) {
                let _ = app.emit("query-progress", result);
            }
        };
        match core::scan(g.as_ref(), &spec, run_id.clone(), &cancel, publish) {
            Ok(result) => publish(result),
            Err(e) => publish(ScanResult {
                run_id,
                status: core::ScanStatus::Error,
                scanned: 0,
                pages: 0,
                exact: false,
                detail: e,
                posts: Vec::new(),
            }),
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transition_invalidates_scan_and_failed_close_keeps_session_epoch() {
        let s = AppState::default();
        s.epoch.store(7, Ordering::SeqCst);
        *s.profile.lock().unwrap() = Some("p-a".into());
        let cancel = Arc::new(AtomicBool::new(false));
        *s.active.lock().unwrap() = Some(("old".into(), cancel.clone()));
        s.closing.store(true, Ordering::SeqCst);
        clear_session(&s);
        assert!(cancel.load(Ordering::Relaxed));
        assert!(s.active.lock().unwrap().is_none());
        assert!(s.results.lock().unwrap().is_none());
        assert!(finish_close(&s, Err("synthetic shutdown failure".into())).is_err());
        assert!(s.closing.load(Ordering::SeqCst));
        assert_eq!(s.epoch.load(Ordering::SeqCst), 7);
        assert!(gateway(&s).is_err());
        let c = finish_close(&s, Ok(())).unwrap();
        assert_eq!(c.generation, 8);
        assert!(!current_epoch(&s, 7));
        assert!(current_epoch(&s, 8));
        assert!(!s.closing.load(Ordering::SeqCst));
    }
    #[test]
    fn pending_sends_and_submission_block_transitions() {
        let s = AppState::default();
        s.pending
            .lock()
            .unwrap()
            .insert("1:2".into(), "Pending".into());
        assert!(can_transition(&s).is_err());
        clear_session(&s);
        assert!(can_transition(&s).is_err());
        s.pending.lock().unwrap().clear();
        s.forwarding.store(true, Ordering::SeqCst);
        assert!(can_transition(&s).is_err());
        s.forwarding.store(false, Ordering::SeqCst);
        assert!(can_transition(&s).is_ok());
    }
    #[test]
    fn old_run_cannot_replace_new_results_or_forward_stale_selection() {
        let s = AppState::default();
        *s.active.lock().unwrap() = Some(("new".into(), Arc::new(AtomicBool::new(false))));
        let old = ScanResult {
            run_id: "old".into(),
            status: core::ScanStatus::Complete,
            scanned: 50,
            pages: 2,
            exact: true,
            detail: String::new(),
            posts: Vec::new(),
        };
        assert!(!retain_result(&s, &old, 0));
        assert!(s.results.lock().unwrap().is_none());
        let current = ScanResult {
            run_id: "new".into(),
            status: core::ScanStatus::Scanning,
            ..old.clone()
        };
        assert!(retain_result(&s, &current, 0));
        assert!(selection(&s, "new", &[]).is_err());
        let current = ScanResult {
            status: core::ScanStatus::Complete,
            ..current
        };
        assert!(retain_result(&s, &current, 0));
        assert!(!retain_result(&s, &old, 0));
        assert_eq!(s.results.lock().unwrap().as_ref().unwrap().run_id, "new");
        assert!(selection(&s, "old", &[]).is_err());
        assert!(selection(&s, "new", &["missing-post".into()]).is_err());
        s.epoch.store(1, Ordering::SeqCst);
        assert!(!retain_result(&s, &current, 0));
        assert!(retain_result(&s, &current, 1));
    }
}
#[tauri::command]
fn cancel_query(s: State<AppState>, run_id: String) {
    if let Some((id, c)) = s.active.lock().unwrap().as_ref()
        && id == &run_id
    {
        c.store(true, Ordering::Relaxed);
    }
}
#[tauri::command]
async fn message_link(app: tauri::AppHandle, key: MessageKey, album: bool) -> Result<String> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ActionService::link(gateway(&s)?.as_ref(), &key, album)
    })
    .await
}
#[tauri::command]
async fn preview_forward(
    app: tauri::AppHandle,
    target: String,
    post_keys: Vec<String>,
    run_id: String,
) -> Result<ForwardReport> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        let keys = selection(&s, &run_id, &post_keys)?;
        ActionService::preview(gateway(&s)?.as_ref(), &target, &keys)
    })
    .await
}
#[tauri::command]
async fn forward_selected(
    app: tauri::AppHandle,
    target: String,
    post_keys: Vec<String>,
    run_id: String,
    confirmed: bool,
) -> Result<ForwardReport> {
    if !confirmed {
        return Err("Explicit confirmation is required".into());
    }
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        if s.forwarding.swap(true, Ordering::SeqCst) {
            return Err("Forwarding is already running".into());
        }
        let r = (|| {
            let keys = selection(&s, &run_id, &post_keys)?;
            let mut report = ActionService::forward(gateway(&s)?.as_ref(), &target, &keys)?;
            let updates = s.sent_updates.lock().unwrap();
            let mut pending = s.pending.lock().unwrap();
            for item in &mut report.items {
                if let Some(id) = &item.sent_id
                    && let Some(state) = updates.get(&format!("{target}:{id}"))
                {
                    item.state = state.clone();
                }
                if item.state == "Pending"
                    && let Some(id) = &item.sent_id
                {
                    pending.insert(format!("{target}:{id}"), "Pending".into());
                }
            }
            Ok(report)
        })();
        s.forwarding.store(false, Ordering::SeqCst);
        r
    })
    .await
}
fn begin_safe_exit(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let s = handle.state::<AppState>();
        let Ok(_lock) = s.lifecycle.try_lock() else {
            let _ = handle.emit("app-error", "会话操作进行中；请等待后重试关闭窗口");
            return;
        };
        match close_session(&handle, false) {
            Ok(()) => handle.exit(0),
            Err(e) => {
                let _ = handle.emit("app-error", e);
            }
        }
    });
}
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<AppState>().gateway.lock().unwrap().is_some() {
                    api.prevent_close();
                    begin_safe_exit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_settings,
            save_settings,
            save_layout,
            reset_settings,
            clear_secret,
            add_account,
            rename_account,
            remove_account,
            logout_account,
            connection_status,
            connect,
            auth_submit,
            disconnect,
            chats,
            public_chat,
            saved_messages,
            start_query,
            cancel_query,
            message_link,
            preview_forward,
            forward_selected
        ])
        .build(tauri::generate_context!())
        .expect("Unable to build TMsgPicker");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let s = app.state::<AppState>();
            if s.gateway.lock().unwrap().is_some() {
                api.prevent_exit();
                begin_safe_exit(app);
            }
        }
    });
}
