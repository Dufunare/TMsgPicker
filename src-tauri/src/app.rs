use crate::{
    core::{self, MessageKey, QuerySpec, ScanResult},
    gateway::{Result, TelegramGateway},
    services::{ActionService, AuthService, Chat, ChatService, ConnectConfig, ForwardReport},
    tdlib::gateway::TdlibGateway,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Serialize)]
struct Connection {
    auth_state: String,
    connection_state: String,
    detail: String,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            auth_state: "Disconnected".into(),
            connection_state: "Disconnected".into(),
            detail: "请输入配置并连接".into(),
        }
    }
}
#[derive(Default)]
struct AppState {
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
fn retain_result(s: &AppState, result: &ScanResult) -> bool {
    let active = s.active.lock().unwrap();
    if active.as_ref().map(|(id, _)| id) != Some(&result.run_id) {
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
fn update(app: &tauri::AppHandle, v: Value) {
    let s = app.state::<AppState>();
    if v["@type"] == "updateAuthorizationState" || v["@type"] == "updateConnectionState" {
        let mut connection = s.connection.lock().unwrap();
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
        let _ = app.emit("connection", connection.clone());
    }
    if v["@type"] == "updateMessageSendSucceeded" || v["@type"] == "updateMessageSendFailed" {
        let target = v["message"]["chat_id"].as_i64().unwrap_or(0).to_string();
        let old = v["old_message_id"].as_i64().unwrap_or(0).to_string();
        let state = if v["@type"] == "updateMessageSendSucceeded" {
            "Sent"
        } else {
            "Failed"
        };
        s.sent_updates
            .lock()
            .unwrap()
            .insert(format!("{target}:{old}"), state.into());
        let _ = app.emit(
            "forward-update",
            json!({"target":target,"old_message_id":old,"state":state}),
        );
    }
}
#[tauri::command]
fn connection_status(s: State<AppState>) -> Connection {
    s.connection.lock().unwrap().clone()
}
#[tauri::command]
async fn connect(app: tauri::AppHandle, config: ConnectConfig) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        if s.connecting.swap(true, Ordering::SeqCst) {
            return Err("Connection setup already running".into());
        }
        let r = (|| {
            let existing = s.gateway.lock().unwrap().clone();
            let g = if let Some(g) = existing {
                g
            } else {
                let handle = app.clone();
                let g = TdlibGateway::new(move |v| update(&handle, v))?;
                *s.gateway.lock().unwrap() = Some(g.clone());
                g
            };
            let state = g.request(
                json!({"@type":"getAuthorizationState"}),
                &AtomicBool::new(false),
            )?;
            update(
                &app,
                json!({"@type":"updateAuthorizationState","authorization_state":state}),
            );
            if state["@type"] != "authorizationStateWaitTdlibParameters" {
                return Err(
                    "Client is already initialized; disconnect before changing configuration"
                        .into(),
                );
            }
            // Match the validated CLI location so the existing session is reused.
            let root = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .ok_or("LOCALAPPDATA is not available")?
                .join("com.funare.tmsgpicker")
                .join("tdlib");
            AuthService::initialize(g.as_ref(), config, &root)
        })();
        s.connecting.store(false, Ordering::SeqCst);
        r
    })
    .await
}
#[tauri::command]
async fn auth_submit(app: tauri::AppHandle, value: String) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let state = s.connection.lock().unwrap().auth_state.clone();
        AuthService::submit(gateway(&s)?.as_ref(), &state, value)
    })
    .await
}
#[tauri::command]
async fn disconnect(app: tauri::AppHandle) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        if let Some((_, c)) = s.active.lock().unwrap().as_ref() {
            c.store(true, Ordering::Relaxed);
        }
        let g = gateway(&s)?;
        g.shutdown()?;
        *s.gateway.lock().unwrap() = None;
        *s.results.lock().unwrap() = None;
        Ok(())
    })
    .await
}
#[tauri::command]
async fn chats(app: tauri::AppHandle, search: String) -> Result<Vec<Chat>> {
    background(move || ChatService::list(gateway(&app.state())?.as_ref(), &search)).await
}
#[tauri::command]
async fn public_chat(app: tauri::AppHandle, username: String) -> Result<Chat> {
    background(move || ChatService::public(gateway(&app.state())?.as_ref(), &username)).await
}
#[tauri::command]
async fn saved_messages(app: tauri::AppHandle) -> Result<Chat> {
    background(move || ChatService::saved(gateway(&app.state())?.as_ref())).await
}
#[tauri::command]
fn start_query(
    app: tauri::AppHandle,
    s: State<AppState>,
    spec: QuerySpec,
    run_id: String,
) -> Result<()> {
    spec.validate()?;
    if run_id.is_empty() || run_id.len() > 100 {
        return Err("Invalid run ID".into());
    }
    let g = gateway(&s)?;
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
            if retain_result(&s, &result) {
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
        assert!(!retain_result(&s, &old));
        assert!(s.results.lock().unwrap().is_none());
        let current = ScanResult {
            run_id: "new".into(),
            status: core::ScanStatus::Scanning,
            ..old.clone()
        };
        assert!(retain_result(&s, &current));
        assert!(selection(&s, "new", &[]).is_err());
        let current = ScanResult {
            status: core::ScanStatus::Complete,
            ..current
        };
        assert!(retain_result(&s, &current));
        assert!(!retain_result(&s, &old));
        assert_eq!(s.results.lock().unwrap().as_ref().unwrap().run_id, "new");
        assert!(selection(&s, "old", &[]).is_err());
        assert!(selection(&s, "new", &["missing-post".into()]).is_err());
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
    background(move || ActionService::link(gateway(&app.state())?.as_ref(), &key, album)).await
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
        if s.forwarding.swap(true, Ordering::SeqCst) {
            return Err("Forwarding is already running".into());
        }
        let r = (|| {
            let keys = selection(&s, &run_id, &post_keys)?;
            let mut report = ActionService::forward(gateway(&s)?.as_ref(), &target, &keys)?;
            let updates = s.sent_updates.lock().unwrap();
            for item in &mut report.items {
                if let Some(id) = &item.sent_id
                    && let Some(state) = updates.get(&format!("{target}:{id}"))
                {
                    item.state = state.clone();
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
    let s = app.state::<AppState>();
    if s.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some((_, c)) = s.active.lock().unwrap().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let s = handle.state::<AppState>();
        let g = s.gateway.lock().unwrap().clone();
        if let Some(g) = g
            && let Err(e) = g.shutdown()
        {
            s.closing.store(false, Ordering::SeqCst);
            let _ = handle.emit("app-error", e);
            return;
        }
        *s.gateway.lock().unwrap() = None;
        handle.exit(0);
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
