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
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Serialize)]
struct Connection {
    profile_id: Option<String>,
    generation: u64,
    runtime_state: String,
    auth_state: String,
    connection_state: String,
    detail: String,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            profile_id: None,
            generation: 0,
            runtime_state: "Closed".into(),
            auth_state: "Disconnected".into(),
            connection_state: "Disconnected".into(),
            detail: "请输入配置并连接".into(),
        }
    }
}
#[derive(Default)]
struct AppState {
    lifecycle: RwLock<()>,
    settings_window: Mutex<()>,
    store: Mutex<Option<Store>>,
    manager: TdlibRuntimeManager,
    exiting: AtomicBool,
}
#[derive(Default)]
struct TdlibRuntimeManager {
    accounts: Mutex<HashMap<String, Arc<AccountRuntime>>>,
}
#[derive(Default)]
struct AccountRuntime {
    lifecycle: Mutex<()>,
    profile_id: String,
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
    unlocked_passphrase: Mutex<Option<String>>,
}
fn account(s: &AppState, id: &str) -> Result<Arc<AccountRuntime>> {
    if s.exiting.load(Ordering::SeqCst) {
        return Err("Application is shutting down".into());
    }
    s.manager
        .accounts
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("Unknown account".into())
}
fn accounts(s: &AppState) -> Vec<Arc<AccountRuntime>> {
    s.manager
        .accounts
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}
fn client(s: &AccountRuntime) -> Result<Arc<TdlibGateway>> {
    if s.closing.load(Ordering::SeqCst) || s.connecting.load(Ordering::SeqCst) {
        return Err("Session transition in progress".into());
    }
    s.gateway
        .lock()
        .unwrap()
        .clone()
        .ok_or("Connect to Telegram first".into())
}
fn gateway(s: &AccountRuntime) -> Result<Arc<TdlibGateway>> {
    if s.connection.lock().unwrap().runtime_state != "Ready" {
        return Err("Account is not ready; unlock or finish authorization first".into());
    }
    client(s)
}
fn selection(s: &AccountRuntime, run_id: &str, post_keys: &[String]) -> Result<Vec<MessageKey>> {
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
fn retain_result(s: &AccountRuntime, result: &ScanResult, epoch: u64) -> bool {
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
fn apply_connection(s: &AccountRuntime, epoch: u64, v: &Value) -> Option<(Connection, bool)> {
    if v["@type"] != "updateAuthorizationState" && v["@type"] != "updateConnectionState" {
        return None;
    }
    let mut connection = s.connection.lock().unwrap();
    if !current_epoch(s, epoch) {
        return None;
    }
    connection.generation = epoch;
    connection.profile_id = Some(s.profile_id.clone());
    if v["@type"] == "updateAuthorizationState" {
        connection.auth_state = v["authorization_state"]["@type"]
            .as_str()
            .unwrap_or("Unknown")
            .into();
        if connection.runtime_state != "NeedsDatabasePassphrase"
            && connection.runtime_state != "Error"
        {
            connection.runtime_state = match connection.auth_state.as_str() {
                "authorizationStateReady" => "Ready",
                "authorizationStateClosing" => "Closing",
                "authorizationStateClosed" => "Closed",
                "authorizationStateWaitTdlibParameters" => "Initializing",
                _ => "AuthorizationRequired",
            }
            .into();
        }
        let detail = match connection.auth_state.as_str() {
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
        if connection.runtime_state != "Error" {
            connection.detail = detail;
        }
    } else {
        connection.connection_state = v["state"]["@type"].as_str().unwrap_or("Unknown").into();
    }
    if connection.runtime_state == "NeedsDatabasePassphrase" {
        connection.detail = "需要此账号数据库口令".into();
    }
    let ready = connection.auth_state == "authorizationStateReady"
        && v["@type"] == "updateAuthorizationState";
    Some((connection.clone(), ready))
}
fn update(app: &tauri::AppHandle, profile_id: &str, epoch: u64, v: Value) {
    let global = app.state::<AppState>();
    // Receive callbacks must remain active while shutdown refuses new business work.
    let s = global
        .manager
        .accounts
        .lock()
        .unwrap()
        .get(profile_id)
        .cloned();
    let Some(s) = s else {
        return;
    };
    if !current_epoch(&s, epoch) {
        return;
    }
    if let Some((connection, ready)) = apply_connection(&s, epoch, &v) {
        let _ = app.emit("connection", connection);
        if ready {
            let handle = app.clone();
            let account_id = profile_id.to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                let global = handle.state::<AppState>();
                let Ok(s) = account(&global, &account_id) else {
                    return;
                };
                let id = Some(s.profile_id.clone());
                let g = s.gateway.lock().unwrap().clone();
                if let Some(g) = g {
                    let me = g.request(json!({"@type":"getMe"}), &AtomicBool::new(false));
                    if !current_epoch(&s, epoch) {
                        return;
                    }
                    if let Ok(me) = me
                        && let Some(id) = id
                    {
                        let outcome = with_store(&global, |store| {
                            if !current_epoch(&s, epoch) {
                                return Ok(false);
                            }
                            store.identify(&id, &me)
                        });
                        match outcome {
                            Ok(duplicate) => {
                                let _ = handle.emit("accounts-changed", ());
                                if duplicate {
                                    // Resolve conflict without changing the registered identity or deleting data.
                                    let _gate = global.lifecycle.read().unwrap();
                                    let _lock = s.lifecycle.lock().unwrap();
                                    if current_epoch(&s, epoch) {
                                        let closed = close_session(&handle, &s, false);
                                        let detail = match closed {
                                            Ok(()) => "重复 Telegram 用户；冲突账号已安全断开，请显式移除多余 Profile".to_string(),
                                            Err(e) => format!("重复 Telegram 用户；安全断开未完成：{e}。请等待后重试断开并移除冲突 Profile"),
                                        };
                                        set_runtime_state(&handle, &s, "Error", &detail);
                                        let _ = handle.emit(
                                            "account-notice",
                                            json!({"profile_id":account_id,"detail":detail}),
                                        );
                                    }
                                }
                            }
                            Err(_) => {
                                let _ = handle.emit(
                                    "account-notice",
                                    json!({"profile_id":account_id,"detail":"账号身份已读取但保存失败；请检查配置目录。"}),
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
            json!({"profile_id":profile_id,"target":target,"old_message_id":old,"state":state,"generation":epoch}),
        );
    }
}
#[tauri::command]
fn connection_status(s: State<AppState>, profile_id: String) -> Result<Connection> {
    Ok(account(&s, &profile_id)?.connection.lock().unwrap().clone())
}
#[tauri::command]
fn account_statuses(s: State<AppState>) -> Vec<Connection> {
    accounts(&s)
        .iter()
        .map(|r| r.connection.lock().unwrap().clone())
        .collect()
}
#[tauri::command]
fn select_profile(s: State<AppState>, profile_id: String) -> Result<()> {
    with_store(&s, |store| store.select(&profile_id))
}
fn set_runtime_state(app: &tauri::AppHandle, s: &AccountRuntime, state: &str, detail: &str) {
    let mut c = s.connection.lock().unwrap();
    c.runtime_state = state.into();
    c.detail = detail.into();
    let _ = app.emit("connection", c.clone());
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
async fn open_settings(app: tauri::AppHandle, profile_id: Option<String>) -> Result<()> {
    // Build WebView2 off the synchronous command thread; serialize duplicate clicks.
    background(move || {
        let s = app.state::<AppState>();
        let _window_lock = s.settings_window.lock().unwrap();
        if let Some(id) = &profile_id {
            with_store(&s, |store| store.profile(id).map(|_| ()))?;
        }
        if let Some(window) = app.get_webview_window("settings") {
            window
                .emit("settings-focus-profile", &profile_id)
                .map_err(|_| "Cannot update Settings window")?;
            window
                .unminimize()
                .and_then(|_| window.show())
                .and_then(|_| window.set_focus())
                .map_err(|_| "Cannot focus Settings window")?;
        } else {
            let url = match profile_id {
                Some(id) => format!("index.html?view=settings&profile={id}"),
                None => "index.html?view=settings".into(),
            };
            tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App(url.into()))
                .title("设置")
                .inner_size(500.0, 580.0)
                .min_inner_size(420.0, 440.0)
                .maximizable(false)
                .center()
                .build()
                .map_err(|_| "Cannot open Settings window")?;
        }
        Ok(())
    })
    .await
}
#[tauri::command]
async fn close_settings(app: tauri::AppHandle) -> Result<()> {
    background(move || {
        if let Some(window) = app.get_webview_window("settings") {
            window.close().map_err(|_| "Cannot close Settings window")?;
        }
        Ok(())
    })
    .await
}
#[tauri::command]
fn save_ui_preferences(s: State<AppState>, card_compact: bool) -> Result<()> {
    with_store(&s, |store| store.set_card_compact(card_compact))
}
fn route_window_close(label: &str, main_close: impl FnOnce()) {
    if label == "main" {
        main_close();
    }
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
fn rebuild_accounts(app: &tauri::AppHandle) -> Result<()> {
    let global = app.state::<AppState>();
    let runtimes = accounts(&global);
    let handles = runtimes
        .into_iter()
        .map(|s| {
            let handle = app.clone();
            std::thread::spawn(move || {
                let id = s.profile_id.clone();
                set_runtime_state(
                    &handle,
                    &s,
                    "NeedsDatabasePassphrase",
                    "Needs this account's database passphrase",
                );
                if let Err(e) = create_client(&handle, &s) {
                    set_runtime_state(&handle, &s, "Error", &e);
                    return;
                }
                let remembered = with_store(&handle.state::<AppState>(), |store| {
                    Ok(store.has(&format!("db:{id}")))
                })
                .unwrap_or(false);
                let passphrase = s.unlocked_passphrase.lock().unwrap().clone();
                if (passphrase.is_some() || remembered)
                    && let Err(e) = initialize_account(
                        &handle,
                        ActivateConfig {
                            profile_id: id,
                            passphrase,
                            remember_database: remembered,
                        },
                    )
                {
                    set_runtime_state(&handle, &s, "Error", &e);
                }
            })
        })
        .collect::<Vec<_>>();
    for h in handles {
        h.join().map_err(|_| "Account rebuild worker failed")?;
    }
    Ok(())
}
#[tauri::command]
async fn save_settings(app: tauri::AppHandle, edit: SettingsEdit) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        if s.exiting.load(Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        // Validate before interrupting any clients.
        Store::validate_settings(&edit.settings)?;
        close_all(&app)?;
        let snapshot = with_store(&s, |store| {
            // Preserve the latest UI preferences instead of writing the editor's stale snapshot.
            let mut settings = edit.settings;
            let latest = store.settings();
            settings.left_ratio = latest.left_ratio;
            settings.right_ratio = latest.right_ratio;
            settings.card_compact = latest.card_compact;
            settings.last_profile_id = latest.last_profile_id;
            store.edit_application_config(settings, edit.api_hash, edit.proxy_password)?;
            Ok(store.snapshot())
        })?;
        rebuild_accounts(&app)?;
        let _ = app.emit("accounts-changed", ());
        Ok(snapshot)
    })
    .await
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
async fn reset_settings(app: tauri::AppHandle) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        close_all(&app)?;
        let snapshot = with_store(&s, |store| {
            store.reset()?;
            Ok(store.snapshot())
        })?;
        rebuild_accounts(&app)?;
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
async fn clear_secret(app: tauri::AppHandle, key: String) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let global_key = key == "api" || key == "proxy";
        let _gate = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        if global_key {
            close_all(&app)?;
        }
        let snapshot = with_store(&s, |store| {
            store.set_secret(&key, None)?;
            if key == "proxy" {
                let mut settings = store.settings();
                settings.proxy_username.clear();
                store.set_settings(settings)?;
            }
            Ok(store.snapshot())
        })?;
        if global_key {
            rebuild_accounts(&app)?;
        }
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
fn add_account(app: tauri::AppHandle, name: String) -> Result<String> {
    let s = app.state::<AppState>();
    let _lock = s
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    if s.exiting.load(Ordering::SeqCst) {
        return Err("Application is shutting down".into());
    }
    let id = with_store(&s, |store| store.add(name))?;
    create_runtime(&app, &id)?;
    Ok(id)
}
#[tauri::command]
fn rename_account(s: State<AppState>, profile_id: String, name: String) -> Result<Snapshot> {
    with_store(&s, |store| {
        store.rename(&profile_id, name)?;
        Ok(store.snapshot())
    })
}
#[tauri::command]
async fn remove_account(
    app: tauri::AppHandle,
    profile_id: String,
    confirmed: bool,
    delete_data: bool,
    delete_confirmed: bool,
) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_write()
            .map_err(|_| "Session operation in progress")?;
        if !confirmed {
            return Err("Confirm local profile removal".into());
        }
        if delete_data && !delete_confirmed {
            return Err("Second confirmation required for Session deletion".into());
        }
        let runtime = account(&s, &profile_id)?;
        let _account_lock = runtime
            .lifecycle
            .try_lock()
            .map_err(|_| "Account operation in progress")?;
        close_session(&app, &runtime, false)?;
        // Session deletion is opt-in and separately confirmed; legacy deletion is refused.
        with_store(&s, |store| {
            if delete_data {
                if !delete_confirmed {
                    return Err("Second confirmation required for Session deletion".into());
                }
                store.delete_session(&profile_id)?;
            }
            store.remove(&profile_id)?;
            s.manager.accounts.lock().unwrap().remove(&profile_id);
            Ok(store.snapshot())
        })
    })
    .await
}
fn clear_session(s: &AccountRuntime) {
    if let Some((_, cancel)) = s.active.lock().unwrap().take() {
        cancel.store(true, Ordering::Relaxed);
    }
    *s.results.lock().unwrap() = None;
    s.sent_updates.lock().unwrap().clear();
}
fn can_transition(s: &AccountRuntime) -> Result<()> {
    if s.forwarding.load(Ordering::SeqCst) || !s.pending.lock().unwrap().is_empty() {
        return Err("仍有正在提交或 Pending 的转发；等待最终状态后再断开或修改全局配置".into());
    }
    Ok(())
}
fn current_epoch(s: &AccountRuntime, epoch: u64) -> bool {
    s.epoch.load(Ordering::SeqCst) == epoch
}
fn finish_close(s: &AccountRuntime, outcome: Result<()>) -> Result<Connection> {
    outcome?;
    *s.gateway.lock().unwrap() = None;
    s.epoch.fetch_add(1, Ordering::SeqCst);
    let c = Connection {
        profile_id: Some(s.profile_id.clone()),
        generation: s.epoch.load(Ordering::SeqCst),
        ..Connection::default()
    };
    *s.connection.lock().unwrap() = c.clone();
    s.closing.store(false, Ordering::SeqCst);
    Ok(c)
}
fn close_session(app: &tauri::AppHandle, s: &AccountRuntime, logout: bool) -> Result<()> {
    can_transition(s)?;
    s.closing.store(true, Ordering::SeqCst);
    set_runtime_state(app, s, "Closing", "正在安全关闭此账号");
    clear_session(s);
    let outcome = (|| {
        let g = s.gateway.lock().unwrap().clone();
        if let Some(g) = g {
            AuthService::end_session(g.as_ref(), logout)?;
        }
        Ok(())
    })();
    let c = match finish_close(s, outcome) {
        Ok(c) => c,
        Err(e) => {
            set_runtime_state(app, s, "Error", &e);
            return Err(e);
        }
    };
    *s.unlocked_passphrase.lock().unwrap() = None;
    let _ = app.emit("connection", c);
    Ok(())
}
#[derive(Deserialize)]
struct ActivateConfig {
    profile_id: String,
    passphrase: Option<String>,
    remember_database: bool,
}
fn create_runtime(app: &tauri::AppHandle, id: &str) -> Result<Arc<AccountRuntime>> {
    let global = app.state::<AppState>();
    with_store(&global, |store| store.profile(id).map(|_| ()))?;
    let mut map = global.manager.accounts.lock().unwrap();
    if let Some(r) = map.get(id) {
        return Ok(r.clone());
    }
    let r = Arc::new(AccountRuntime {
        profile_id: id.into(),
        ..AccountRuntime::default()
    });
    *r.connection.lock().unwrap() = Connection {
        profile_id: Some(id.into()),
        runtime_state: "NeedsDatabasePassphrase".into(),
        detail: "需要此账号数据库口令".into(),
        ..Connection::default()
    };
    // Publish before native callbacks; callback blocks on map only until registration ends.
    map.insert(id.into(), r.clone());
    drop(map);
    if let Err(e) = create_client(app, &r) {
        set_runtime_state(app, &r, "Error", &e);
    }
    Ok(r)
}
fn create_client(app: &tauri::AppHandle, s: &AccountRuntime) -> Result<()> {
    let id = s.profile_id.clone();
    let epoch = {
        let mut c = s.connection.lock().unwrap();
        let epoch = s.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        c.generation = epoch;
        c.auth_state = "authorizationStateWaitTdlibParameters".into();
        c.connection_state = "Disconnected".into();
        epoch
    };
    let handle = app.clone();
    let callback_id = id.clone();
    let g = TdlibGateway::new(move |v| update(&handle, &callback_id, epoch, v))?;
    *s.gateway.lock().unwrap() = Some(g.clone());
    g.start_authorization_updates();
    Ok(())
}
fn initialize_account(app: &tauri::AppHandle, config: ActivateConfig) -> Result<()> {
    let global = app.state::<AppState>();
    let s = account(&global, &config.profile_id)?;
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Account operation in progress")?;
    if s.closing.load(Ordering::SeqCst) {
        return Err("Account is closing; retry disconnect first".into());
    }
    let state = s.connection.lock().unwrap().auth_state.clone();
    if s.gateway
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|g| !g.is_closed())
        && state != "authorizationStateWaitTdlibParameters"
    {
        return Err("Account already initialized; finish authorization or disconnect first".into());
    }
    let (root, c) = with_store(&global, |store| {
        let credentials = store.account_credentials(&config.profile_id, config.passphrase)?;
        let root = credentials.session_root;
        let settings = store.settings();
        let passphrase = credentials.passphrase.ok_or("需要此账号数据库口令")?;
        let api_hash = store.secret("api")?;
        let password = if settings.proxy_enabled {
            store.secret("proxy")?.unwrap_or_default()
        } else {
            String::new()
        };
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
    let recreate = s
        .gateway
        .lock()
        .unwrap()
        .as_ref()
        .is_none_or(|g| g.is_closed());
    if recreate {
        create_client(app, &s)?;
    }
    s.connecting.store(true, Ordering::SeqCst);
    set_runtime_state(app, &s, "Initializing", "正在初始化此账号");
    let g = s
        .gateway
        .lock()
        .unwrap()
        .clone()
        .ok_or("Client unavailable")?;
    let key = c.passphrase.clone();
    let outcome = AuthService::initialize(g.as_ref(), c, &root).and_then(|()| {
        *s.unlocked_passphrase.lock().unwrap() = Some(key.clone());
        // Remember a new key only after TDLib accepts it for this profile's database.
        with_store(&global, |store| {
            store.set_secret(
                &format!("db:{}", config.profile_id),
                if config.remember_database {
                    Some(key.clone())
                } else {
                    None
                },
            )
        })
    });
    if outcome.is_ok() {
        *s.unlocked_passphrase.lock().unwrap() = Some(key);
    }
    s.connecting.store(false, Ordering::SeqCst);
    if let Err(e) = &outcome {
        set_runtime_state(app, &s, "Error", e);
    } else {
        let mut connection = s.connection.lock().unwrap();
        connection.runtime_state = if connection.auth_state == "authorizationStateReady" {
            "Ready"
        } else {
            "AuthorizationRequired"
        }
        .into();
        let _ = app.emit("connection", connection.clone());
    }
    outcome
}
fn start_accounts(app: &tauri::AppHandle) -> Result<()> {
    let global = app.state::<AppState>();
    let ids = with_store(&global, |store| Ok(store.profile_ids()))?;
    // Create all clients before starting any blocking bootstrap.
    for id in &ids {
        create_runtime(app, id)?;
    }
    for id in ids {
        let handle = app.clone();
        let initial_epoch = account(&global, &id)?.epoch.load(Ordering::SeqCst);
        std::thread::spawn(move || {
            let global = handle.state::<AppState>();
            let Ok(_gate) = global.lifecycle.read() else {
                return;
            };
            if global.exiting.load(Ordering::SeqCst) {
                return;
            }
            let Ok(runtime) = account(&global, &id) else {
                return;
            };
            if !current_epoch(&runtime, initial_epoch)
                || runtime.connection.lock().unwrap().runtime_state != "NeedsDatabasePassphrase"
            {
                return;
            }
            let remembered =
                with_store(&global, |store| Ok(store.has(&format!("db:{id}")))).unwrap_or(false);
            if remembered
                && let Err(e) = initialize_account(
                    &handle,
                    ActivateConfig {
                        profile_id: id.clone(),
                        passphrase: None,
                        remember_database: true,
                    },
                )
                && let Ok(s) = account(&global, &id)
            {
                set_runtime_state(&handle, &s, "Error", &e);
            }
        });
    }
    Ok(())
}
#[tauri::command]
async fn connect(app: tauri::AppHandle, config: ActivateConfig) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        initialize_account(&app, config)
    })
    .await
}
#[tauri::command]
async fn auth_submit(app: tauri::AppHandle, profile_id: String, value: String) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        let state = s.connection.lock().unwrap().auth_state.clone();
        AuthService::submit(client(&s)?.as_ref(), &state, value)
    })
    .await
}
#[tauri::command]
async fn disconnect(app: tauri::AppHandle, profile_id: String) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        close_session(&app, &s, false)
    })
    .await
}
#[tauri::command]
async fn logout_account(app: tauri::AppHandle, profile_id: String, confirmed: bool) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        if !confirmed {
            return Err("Confirm logout of the active account".into());
        }
        if s.connection.lock().unwrap().auth_state != "authorizationStateReady" {
            return Err("Account must be ready to log out".into());
        }
        close_session(&app, &s, true)
    })
    .await
}
#[tauri::command]
async fn chats(app: tauri::AppHandle, profile_id: String, search: String) -> Result<Vec<Chat>> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ChatService::list(gateway(&s)?.as_ref(), &search)
    })
    .await
}
#[tauri::command]
async fn public_chat(app: tauri::AppHandle, profile_id: String, username: String) -> Result<Chat> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .try_lock()
            .map_err(|_| "Session operation in progress")?;
        ChatService::public(gateway(&s)?.as_ref(), &username)
    })
    .await
}
#[tauri::command]
async fn saved_messages(app: tauri::AppHandle, profile_id: String) -> Result<Chat> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
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
    profile_id: String,
    global: State<AppState>,
    spec: QuerySpec,
    run_id: String,
) -> Result<()> {
    let _gate = global
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    let s = account(&global, &profile_id)?;
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Account operation in progress")?;
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
            let global = app.state::<AppState>();
            let s = global
                .manager
                .accounts
                .lock()
                .unwrap()
                .get(&profile_id)
                .cloned();
            let Some(s) = s else {
                return;
            };
            if retain_result(&s, &result, epoch) {
                let _ = app.emit(
                    "query-progress",
                    json!({"profile_id":profile_id,"generation":epoch,"result":result}),
                );
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
    fn authorization_network_lock_errors_and_generations_are_orthogonal() {
        let a = AccountRuntime {
            profile_id: "p-a".into(),
            ..AccountRuntime::default()
        };
        let b = AccountRuntime {
            profile_id: "p-b".into(),
            ..AccountRuntime::default()
        };
        b.connection.lock().unwrap().runtime_state = "NeedsDatabasePassphrase".into();
        let auth =
            |kind| json!({"@type":"updateAuthorizationState","authorization_state":{"@type":kind}});
        let (c, ready) = apply_connection(&a, 0, &auth("authorizationStateReady")).unwrap();
        assert!(ready);
        assert_eq!(c.runtime_state, "Ready");
        assert_eq!(c.connection_state, "Disconnected");
        let (c, ready) = apply_connection(
            &a,
            0,
            &json!({"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}),
        )
        .unwrap();
        assert!(!ready);
        assert_eq!(c.auth_state, "authorizationStateReady");
        assert_eq!(c.runtime_state, "Ready");
        assert_eq!(c.connection_state, "connectionStateReady");
        let (c, _) =
            apply_connection(&b, 0, &auth("authorizationStateWaitTdlibParameters")).unwrap();
        assert_eq!(c.runtime_state, "NeedsDatabasePassphrase");
        assert_eq!(c.connection_state, "Disconnected");
        {
            let mut c = b.connection.lock().unwrap();
            c.runtime_state = "Error".into();
            c.detail = "synthetic key failure".into();
        }
        let (c, _) =
            apply_connection(&b, 0, &auth("authorizationStateWaitTdlibParameters")).unwrap();
        assert_eq!(c.runtime_state, "Error");
        assert_eq!(c.detail, "synthetic key failure");
        assert_eq!(a.connection.lock().unwrap().runtime_state, "Ready");
        a.epoch.store(1, Ordering::SeqCst);
        assert!(apply_connection(&a, 0, &auth("authorizationStateClosed")).is_none());
        assert_eq!(
            a.connection.lock().unwrap().auth_state,
            "authorizationStateReady"
        );
    }
    #[test]
    fn runtimes_and_background_queries_survive_view_selection_independently() {
        let global = AppState::default();
        assert!(accounts(&global).is_empty());
        let a = Arc::new(AccountRuntime {
            profile_id: "p-a".into(),
            ..AccountRuntime::default()
        });
        let b = Arc::new(AccountRuntime {
            profile_id: "p-b".into(),
            ..AccountRuntime::default()
        });
        global
            .manager
            .accounts
            .lock()
            .unwrap()
            .extend([("p-a".into(), a.clone()), ("p-b".into(), b.clone())]);
        assert!(account(&global, "arbitrary-client-id").is_err());
        let cancel_a = Arc::new(AtomicBool::new(false));
        let cancel_b = Arc::new(AtomicBool::new(false));
        *a.active.lock().unwrap() = Some(("same-run".into(), cancel_a.clone()));
        *b.active.lock().unwrap() = Some(("same-run".into(), cancel_b.clone()));
        let result = ScanResult {
            run_id: "same-run".into(),
            status: core::ScanStatus::Complete,
            scanned: 1,
            pages: 1,
            exact: true,
            detail: String::new(),
            posts: Vec::new(),
        };
        assert!(retain_result(&a, &result, 0));
        assert!(b.results.lock().unwrap().is_none());
        assert!(retain_result(&b, &result, 0));
        // Changing the last-view preference cannot touch this runtime map or its tasks.
        for id in ["p-a", "p-b", "p-a"] {
            assert!(Arc::ptr_eq(
                &account(&global, id).unwrap(),
                if id == "p-a" { &a } else { &b }
            ));
        }
        assert!(!cancel_a.load(Ordering::Relaxed));
        assert!(!cancel_b.load(Ordering::Relaxed));
        a.pending
            .lock()
            .unwrap()
            .insert("1:2".into(), "Pending".into());
        assert!(can_transition(&a).is_err());
        assert!(can_transition(&b).is_ok());
        clear_session(&a);
        finish_close(&a, Ok(())).unwrap();
        assert!(cancel_a.load(Ordering::Relaxed));
        assert!(!cancel_b.load(Ordering::Relaxed));
        assert!(b.results.lock().unwrap().is_some());
        assert!(!retain_result(&a, &result, 0));
        assert!(retain_result(&b, &result, 0));
        global.exiting.store(true, Ordering::SeqCst);
        assert!(account(&global, "p-b").is_err());
    }
    #[test]
    fn transition_invalidates_scan_and_failed_close_keeps_session_epoch() {
        let s = AccountRuntime {
            profile_id: "p-a".into(),
            ..AccountRuntime::default()
        };
        s.epoch.store(7, Ordering::SeqCst);
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
    fn auxiliary_window_close_is_not_an_application_exit() {
        let s = AccountRuntime::default();
        s.epoch.store(9, Ordering::SeqCst);
        s.pending
            .lock()
            .unwrap()
            .insert("1:2".into(), "Pending".into());
        for label in ["settings", "other"] {
            route_window_close(label, || {
                panic!("auxiliary close must never call the session exit handler")
            });
        }
        assert_eq!(s.epoch.load(Ordering::SeqCst), 9);
        assert!(!s.closing.load(Ordering::SeqCst));
        assert_eq!(s.pending.lock().unwrap().len(), 1);
        let called = AtomicBool::new(false);
        route_window_close("main", || called.store(true, Ordering::SeqCst));
        assert!(called.load(Ordering::SeqCst));
    }
    #[test]
    fn pending_sends_and_submission_block_transitions() {
        let s = AccountRuntime::default();
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
        let s = AccountRuntime::default();
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
fn cancel_query(global: State<AppState>, profile_id: String, run_id: String) -> Result<()> {
    let s = account(&global, &profile_id)?;
    if let Some((id, c)) = s.active.lock().unwrap().as_ref()
        && id == &run_id
    {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
async fn message_link(
    app: tauri::AppHandle,
    profile_id: String,
    key: MessageKey,
    album: bool,
) -> Result<String> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
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
    profile_id: String,
    target: String,
    post_keys: Vec<String>,
    run_id: String,
) -> Result<ForwardReport> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
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
    profile_id: String,
    target: String,
    post_keys: Vec<String>,
    run_id: String,
    confirmed: bool,
) -> Result<ForwardReport> {
    if !confirmed {
        return Err("Explicit confirmation is required".into());
    }
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
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
fn close_all(app: &tauri::AppHandle) -> Result<()> {
    let global = app.state::<AppState>();
    let runtimes = accounts(&global);
    for s in &runtimes {
        can_transition(s)?;
    }
    // Submit every close before waiting, including locked clients.
    for s in &runtimes {
        s.closing.store(true, Ordering::SeqCst);
        clear_session(s);
        set_runtime_state(app, s, "Closing", "正在安全关闭数据库");
        if let Some(g) = s.gateway.lock().unwrap().clone() {
            g.begin_close();
        }
    }
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = runtimes
            .iter()
            .map(|s| {
                scope.spawn(move || {
                    let g = s.gateway.lock().unwrap().clone();
                    let outcome = g.map_or(Ok(()), |g| g.wait_closed());
                    match finish_close(s, outcome) {
                        Ok(c) => {
                            let _ = app.emit("connection", c);
                            Ok(())
                        }
                        Err(e) => {
                            set_runtime_state(app, s, "Error", &e);
                            Err(e)
                        }
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("Close worker failed".into()))
            })
            .collect::<Vec<_>>()
    });
    let errors: Vec<_> = outcomes.into_iter().filter_map(|r| r.err()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
fn begin_safe_exit(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let s = handle.state::<AppState>();
        if s.exiting.swap(true, Ordering::SeqCst) {
            return;
        }
        let _lock = s.lifecycle.write().unwrap();
        match close_all(&handle) {
            Ok(()) => handle.exit(0),
            Err(e) => {
                s.exiting.store(false, Ordering::SeqCst);
                let _ = handle.emit("app-error", e);
            }
        }
    });
}
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // Register all profiles during setup; initialize in independent workers.
            if let Err(e) = start_accounts(&handle) {
                let _ = handle.emit("app-error", e);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                route_window_close(window.label(), || {
                    api.prevent_close();
                    begin_safe_exit(window.app_handle());
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_settings,
            close_settings,
            save_ui_preferences,
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
            account_statuses,
            select_profile,
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
            if accounts(&s)
                .iter()
                .any(|r| r.gateway.lock().unwrap().is_some())
            {
                api.prevent_exit();
                begin_safe_exit(app);
            }
        }
    });
}
