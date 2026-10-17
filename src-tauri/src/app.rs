use crate::login_code_diagnostics::LoginCodeDiagnostics;
use crate::services::ChatListScope;
use crate::settings::{AppearancePreferences, Settings, Snapshot, Store};
use crate::settings_target::SettingsTarget;
use crate::{
    core::{self, MessageKey, QuerySpec, ScanResult},
    gateway::{Result, SessionLifecycle, TelegramGateway},
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
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
#[path = "login.rs"]
mod login;
use login::*;
#[path = "window_theme.rs"]
mod window_theme;
#[path = "appearance_session.rs"]
mod appearance_session;
use appearance_session::{AppearanceSession, AppearanceSnapshot};

#[derive(Clone, Serialize)]
struct Connection {
    profile_id: Option<String>,
    generation: u64,
    runtime_state: String,
    auth_state: String,
    connection_state: String,
    detail: String,
    user_disconnected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_diagnostics: Option<LoginCodeDiagnostics>,
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
            user_disconnected: false,
            code_diagnostics: None,
        }
    }
}
#[derive(Default)]
struct AppState {
    lifecycle: RwLock<()>,
    settings_window: Mutex<()>,
    settings_target: Mutex<SettingsTarget>,
    appearance_session: Mutex<AppearanceSession>,
    pending_login: Mutex<Option<PendingLogin>>,
    store: Mutex<Option<Store>>,
    manager: TdlibRuntimeManager,
    exiting: AtomicBool,
    reset_cleanup_pending: AtomicBool,
    close_approvals: Mutex<HashMap<String, CloseApproval>>,
    approval_sequence: AtomicU64,
}
struct PendingLogin {
    profile_id: String,
    phone: Option<String>,
    phone_submitting: bool,
    remember_database: bool,
    newly_created: bool,
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
    pending: Mutex<HashMap<String, PendingSend>>,
    unknown_sends: Mutex<Vec<UnknownSend>>,
    gateway: Mutex<Option<Arc<TdlibGateway>>>,
    connecting: AtomicBool,
    closing: AtomicBool,
    user_disconnected: AtomicBool,
    logout_progress: Mutex<crate::services::LogoutProgress>,
    #[cfg(debug_assertions)]
    reset_pending_closed: AtomicBool,
    connection: Mutex<Connection>,
    active: Mutex<Option<(String, Arc<AtomicBool>)>>,
    results: Mutex<Option<ScanResult>>,
    forwarding: AtomicBool,
    sent_updates: Mutex<HashMap<String, String>>,
    unlocked_passphrase: Mutex<Option<String>>,
}
const PENDING_CONFIRM_AFTER: Duration = Duration::from_secs(30);
#[derive(Clone)]
struct PendingSend {
    started_at: Instant,
}
impl Default for PendingSend {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct PendingKey {
    profile_id: String,
    generation: u64,
    key: String,
}
struct CloseApproval {
    entries: Vec<PendingKey>,
    created_at: Instant,
}
#[derive(Clone, Serialize)]
struct CloseChallenge {
    pending_count: usize,
    retry_after_ms: u64,
    token: Option<String>,
}
#[derive(Clone, Serialize)]
struct UnknownSend {
    profile_id: String,
    generation: u64,
    target: String,
    old_message_id: String,
    state: String,
}
fn pending_challenge(
    global: &AppState,
    runtimes: &[Arc<AccountRuntime>],
) -> Result<CloseChallenge> {
    let mut entries = Vec::new();
    let mut wait = Duration::ZERO;
    for s in runtimes {
        if s.forwarding.load(Ordering::SeqCst) {
            return Err("Forward submission in progress; wait before closing".into());
        }
        for (key, pending) in s.pending.lock().unwrap().iter() {
            wait = wait.max(PENDING_CONFIRM_AFTER.saturating_sub(pending.started_at.elapsed()));
            entries.push(PendingKey {
                profile_id: s.profile_id.clone(),
                generation: s.epoch.load(Ordering::SeqCst),
                key: key.clone(),
            });
        }
    }
    let pending_count = entries.len();
    let token = if pending_count > 0 && wait.is_zero() {
        let token = global
            .approval_sequence
            .fetch_add(1, Ordering::SeqCst)
            .to_string();
        let mut approvals = global.close_approvals.lock().unwrap();
        approvals.retain(|_, a| a.created_at.elapsed() < Duration::from_secs(300));
        if approvals.len() >= 32 {
            approvals.clear();
        }
        approvals.insert(
            token.clone(),
            CloseApproval {
                entries,
                created_at: Instant::now(),
            },
        );
        Some(token)
    } else {
        None
    };
    Ok(CloseChallenge {
        pending_count,
        retry_after_ms: u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
        token,
    })
}
#[tauri::command]
fn close_guard(s: State<AppState>, profile_id: Option<String>) -> Result<CloseChallenge> {
    let _gate = s
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    let runtimes = match profile_id {
        Some(id) => vec![account(&s, &id)?],
        None => accounts(&s),
    };
    pending_challenge(&s, &runtimes)
}
#[tauri::command]
fn unknown_sends(s: State<AppState>) -> Vec<UnknownSend> {
    accounts(&s)
        .iter()
        .flat_map(|r| r.unknown_sends.lock().unwrap().clone())
        .collect()
}
fn authorize_close(
    global: &AppState,
    runtimes: &[Arc<AccountRuntime>],
    token: Option<String>,
) -> Result<Vec<UnknownSend>> {
    let approval = match token {
        Some(token) => Some(
            global
                .close_approvals
                .lock()
                .unwrap()
                .remove(&token)
                .ok_or("Close confirmation expired; review pending sends again")?,
        ),
        None => None,
    };
    // Same order as receiver updates. Hold every pending map until the complete
    // snapshot is validated, so an invalid confirmation cannot partially resolve A.
    let mut ordered: Vec<_> = runtimes.iter().collect();
    ordered.sort_by_key(|r| &r.profile_id);
    let mut updates: Vec<_> = ordered
        .iter()
        .map(|r| r.sent_updates.lock().unwrap())
        .collect();
    let mut pending: Vec<_> = ordered.iter().map(|r| r.pending.lock().unwrap()).collect();
    for (r, sends) in ordered.iter().zip(&pending) {
        if r.forwarding.load(Ordering::SeqCst) {
            return Err("Forward submission in progress".into());
        }
        for (key, send) in sends.iter() {
            if key.split_once(':').is_none() {
                return Err("Invalid pending send key".into());
            }
            let approved = approval.as_ref().is_some_and(|a| {
                a.created_at.elapsed() < Duration::from_secs(300)
                    && a.entries.contains(&PendingKey {
                        profile_id: r.profile_id.clone(),
                        generation: r.epoch.load(Ordering::SeqCst),
                        key: key.clone(),
                    })
            });
            if !approved || send.started_at.elapsed() < PENDING_CONFIRM_AFTER {
                return Err(
                    "Pending sends require timeout and explicit close confirmation; review again"
                        .into(),
                );
            }
        }
    }
    let mut unknown = Vec::new();
    for ((r, sends), sent_updates) in ordered.iter().zip(&mut pending).zip(&mut updates) {
        for (key, _) in sends.drain() {
            let (target, old_message_id) = key.split_once(':').ok_or("Invalid pending send key")?;
            sent_updates.insert(key.clone(), "Unknown".into());
            let event = UnknownSend {
                profile_id: r.profile_id.clone(),
                generation: r.epoch.load(Ordering::SeqCst),
                target: target.into(),
                old_message_id: old_message_id.into(),
                state: "Unknown".into(),
            };
            r.unknown_sends.lock().unwrap().push(event.clone());
            unknown.push(event);
        }
    }
    Ok(unknown)
}
fn approve_and_emit(
    app: &tauri::AppHandle,
    runtimes: &[Arc<AccountRuntime>],
    token: Option<String>,
) -> Result<()> {
    for event in authorize_close(&app.state::<AppState>(), runtimes, token)? {
        let _ = app.emit("forward-update", event);
    }
    Ok(())
}
fn account(s: &AppState, id: &str) -> Result<Arc<AccountRuntime>> {
    ensure_running(s)?;
    s.manager
        .accounts
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("Unknown account".into())
}
fn ensure_running(s: &AppState) -> Result<()> {
    if s.reset_cleanup_pending.load(Ordering::SeqCst) {
        return Err("本地重置清理未完成，请重试 Debug 重置或退出应用".into());
    }
    if s.exiting.load(Ordering::SeqCst) {
        return Err("Application is shutting down".into());
    }
    Ok(())
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
fn selection(s: &AccountRuntime, run_id: &str, post_keys: &[String], allow_incomplete: bool) -> Result<Vec<MessageKey>> {
    let results = s.results.lock().unwrap();
    let r = results.as_ref().ok_or("Run a query first")?;
    if r.run_id != run_id || r.status == core::ScanStatus::Running {
        return Err("Query selection is stale or still scanning".into());
    }
    let mut keys = Vec::new();
    for key in post_keys {
        let p = r
            .posts
            .iter()
            .find(|p| &p.key == key)
            .ok_or("Post is no longer in results")?;
        if matches!(p.completeness.level,core::CompletenessLevel::Unknown|core::CompletenessLevel::Truncated)
            && !allow_incomplete {
            return Err("该 Post 可能缺少媒体组成员，必须明确确认后才允许转发".into());
        }
        keys.extend(p.members.iter().map(|m| m.key.clone()));
    }
    Ok(keys)
}
/// Invalidate one run and release its retained ranking without touching the
/// TDLib Session or any send receipts. Lock order matches start_query/publish:
/// active first, then results, so a late event cannot resurrect freed Posts.
fn release_result(s:&AccountRuntime,run_id:&str){
    // Keep the active -> results lock ordering used by start_query/publish,
    // but destroy potentially large Post trees *after* both locks are released.
    let abandoned={
        let mut active=s.active.lock().unwrap();
        if active.as_ref().is_some_and(|(id,_)|id==run_id)
            && let Some((_,cancel))=active.take()
        {
            cancel.store(true,Ordering::Relaxed);
        }
        let mut results=s.results.lock().unwrap();
        if results.as_ref().is_some_and(|result|result.run_id==run_id){
            results.take()
        }else{None}
    };
    drop(abandoned);
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
        // Deliver only safe metadata from authenticationCodeInfo to the login
        // UI. It contains a phone_number that must never cross this IPC.
        connection.code_diagnostics =
            (connection.auth_state == "authorizationStateWaitCode").then(|| {
                LoginCodeDiagnostics::from_wait_code(&v["authorization_state"])
            });
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
    if v["@type"] == "runtimeReceiverFailed" {
        if let Some((_, cancel)) = s.active.lock().unwrap().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        set_runtime_state(
            app,
            &s,
            "Error",
            "Shared TDLib receiver failed; pending requests failed immediately. Retry disconnect to safely close clients.",
        );
        return;
    }
    if let Some((connection, ready)) = apply_connection(&s, epoch, &v) {
        if v["@type"] == "updateAuthorizationState"
            && !matches!(
                connection.auth_state.as_str(),
                "authorizationStateWaitTdlibParameters" | "authorizationStateWaitPhoneNumber"
            )
            && let Some(login) = global
                .pending_login
                .lock()
                .unwrap()
                .as_mut()
                .filter(|p| p.profile_id == profile_id)
        {
            login.phone_submitting = false;
        }
        let _ = app.emit("connection", connection);
        if ready {
            let handle = app.clone();
            let runtime = s.clone();
            tauri::async_runtime::spawn_blocking(move || {
                complete_identity(&handle, &runtime, epoch)
            });
        }
        if v["authorization_state"]["@type"] == "authorizationStateWaitPhoneNumber" {
            let handle = app.clone();
            let runtime = s.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let global = handle.state::<AppState>();
                let _gate = global.lifecycle.read().unwrap();
                let _lock = runtime.lifecycle.lock().unwrap();
                if !current_epoch(&runtime, epoch) || global.exiting.load(Ordering::SeqCst) {
                    return;
                }
                let phone = global
                    .pending_login
                    .lock()
                    .unwrap()
                    .as_mut()
                    .filter(|p| p.profile_id == runtime.profile_id)
                    .and_then(|p| p.phone.take());
                if let Some(phone) = phone {
                    let outcome = client(&runtime).and_then(|g| {
                        AuthService::submit(g.as_ref(), "authorizationStateWaitPhoneNumber", phone)
                    });
                    if let Err(error) = outcome {
                        if let Some(login) = global
                            .pending_login
                            .lock()
                            .unwrap()
                            .as_mut()
                            .filter(|p| p.profile_id == runtime.profile_id)
                        {
                            login.phone_submitting = false;
                        }
                        let _ = handle.emit("login-error", error);
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
        s.unknown_sends
            .lock()
            .unwrap()
            .retain(|u| !(u.generation == epoch && u.target == target && u.old_message_id == old));
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
    let _gate = s
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    ensure_running(&s)?;
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
fn open_auxiliary(app: &tauri::AppHandle, label: &str, url: &str, title: &str) -> Result<()> {
    if let Some(window) = app.get_webview_window(label) {
        window
            .unminimize()
            .and_then(|_| window.show())
            .and_then(|_| window.set_focus())
            .map_err(|_| "Cannot focus window")?;
    } else {
        let window =
            tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(url.into()))
                .title(title)
                .inner_size(500.0, 580.0)
                .min_inner_size(420.0, 440.0)
                .maximizable(false)
                .decorations(false)
                .center()
                .build()
                .map_err(|_| "Cannot open window")?;
        window_theme::apply(&window);
        if let Ok(snapshot) = current_appearance(&app.state::<AppState>()) {
            window_theme::apply_appearance(&window, &snapshot.appearance);
        }
    }
    Ok(())
}
#[tauri::command]
async fn open_settings(
    app: tauri::AppHandle,
    tab: Option<String>,
    profile_id: Option<String>,
) -> Result<()> {
    background(move || {
        let s = app.state::<AppState>();
        let _gate = s
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        if s.exiting.load(Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        if s.reset_cleanup_pending.load(Ordering::SeqCst) {
            return Err("本地重置清理未完成，请重试 Debug 重置".into());
        }
        let _window_lock = s.settings_window.lock().unwrap();
        let target = SettingsTarget::resolve(tab.as_deref(), profile_id, |id| {
            with_store(&s, |store| store.select(id))
        })?;
        *s.settings_target.lock().unwrap() = target.clone();
        open_auxiliary(&app, "settings", "index.html?view=settings", "设置")?;
        if let Some(window) = app.get_webview_window("settings") {
            window
                .emit("settings-navigate", target)
                .map_err(|_| "Cannot navigate settings")?;
        }
        Ok(())
    })
    .await
}
#[tauri::command]
fn settings_target(s: State<AppState>) -> SettingsTarget {
    s.settings_target.lock().unwrap().clone()
}
fn route_window_close(label: &str, main_close: impl FnOnce()) {
    if label == "main" {
        main_close();
    }
}
fn current_appearance(state: &AppState) -> Result<AppearanceSnapshot> {
    let session = state.appearance_session.lock().unwrap();
    with_store(state, |store| Ok(session.snapshot(store.settings().appearance)))
}
fn publish_appearance(app: &tauri::AppHandle, snapshot: &AppearanceSnapshot) {
    for window in app.webview_windows().values() {
        window_theme::apply_appearance(window, &snapshot.appearance);
    }
    let _ = app.emit("appearance-changed", snapshot);
}
#[tauri::command]
fn appearance_state(s: State<AppState>) -> Result<AppearanceSnapshot> {
    let _gate = s
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    ensure_running(&s)?;
    current_appearance(&s)
}
fn edit_appearance(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    editor: u64,
    appearance: AppearancePreferences,
    save: bool,
) -> Result<AppearanceSnapshot> {
    if window.label() != "settings" || app.get_webview_window("settings").is_none() {
        return Err("请在外观设置中调整主题".into());
    }
    let state = app.state::<AppState>();
    let _gate = state
        .lifecycle
        .try_read()
        .map_err(|_| "Application transition in progress")?;
    ensure_running(&state)?;
    let snapshot = {
        let mut session = state.appearance_session.lock().unwrap();
        let persist = |next: &AppearancePreferences| {
            with_store(&state, |store| store.set_appearance(next.clone()))
        };
        session.update(editor, appearance, if save { Some(persist) } else { None })?;
        with_store(&state, |store| Ok(session.snapshot(store.settings().appearance)))?
    };
    publish_appearance(app, &snapshot);
    Ok(snapshot)
}
#[tauri::command]
fn preview_appearance(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    editor: u64,
    appearance: AppearancePreferences,
) -> Result<AppearanceSnapshot> {
    edit_appearance(&app, &window, editor, appearance, false)
}
#[tauri::command]
fn save_appearance(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    editor: u64,
    appearance: AppearancePreferences,
) -> Result<AppearanceSnapshot> {
    edit_appearance(&app, &window, editor, appearance, true)
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
                if s.logout_progress.lock().unwrap().confirmed {
                    set_runtime_state(
                        &handle,
                        &s,
                        "LogoutCleanupRequired",
                        "Telegram 授权已注销，请重试完成本地清理",
                    );
                    return;
                }
                if s.user_disconnected.load(Ordering::SeqCst) {
                    return;
                }
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
async fn save_settings(
    app: tauri::AppHandle,
    edit: SettingsEdit,
    close_confirmation: Option<String>,
) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        if s.exiting.load(Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        if s.reset_cleanup_pending.load(Ordering::SeqCst) {
            return Err("本地重置清理未完成，请重试 Debug 重置".into());
        }
        // Validate before interrupting any clients.
        Store::validate_settings(&edit.settings)?;
        close_all(&app, close_confirmation)?;
        let snapshot = with_store(&s, |store| {
            let mut settings = edit.settings;
            settings.last_profile_id = None;
            // Connection settings cannot commit an appearance draft or overwrite a newer theme.
            settings.appearance = store.settings().appearance;
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
async fn reset_settings(
    app: tauri::AppHandle,
    close_confirmation: Option<String>,
) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let _lock = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        ensure_running(&s)?;
        close_all(&app, close_confirmation)?;
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
async fn clear_secret(
    app: tauri::AppHandle,
    key: String,
    close_confirmation: Option<String>,
) -> Result<Snapshot> {
    background(move || {
        let s = app.state::<AppState>();
        let global_key = key == "api" || key == "proxy";
        let _gate = s
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        ensure_running(&s)?;
        if global_key {
            close_all(&app, close_confirmation)?;
        }
        let snapshot = with_store(&s, |store| {
            store.clear_credential(&key)?;
            Ok(store.snapshot())
        })?;
        if global_key {
            rebuild_accounts(&app)?;
        }
        Ok(snapshot)
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
    if let Ok(mut progress) = s.logout_progress.try_lock()
        && progress.confirmed
    {
        progress.closed = true;
    }
    *s.gateway.lock().unwrap() = None;
    s.epoch.fetch_add(1, Ordering::SeqCst);
    let c = Connection {
        profile_id: Some(s.profile_id.clone()),
        generation: s.epoch.load(Ordering::SeqCst),
        user_disconnected: s.user_disconnected.load(Ordering::SeqCst),
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
    let logout_completed = with_store(&global, |store| Ok(store.profile(id)?.logout_completed))?;
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
    if logout_completed {
        *r.logout_progress.lock().unwrap() = crate::services::LogoutProgress {
            confirmed: true,
            closed: true,
            recorded: true,
        };
        set_runtime_state(
            app,
            &r,
            "LogoutCleanupRequired",
            "上次已确认注销，请重试完成本地清理",
        );
        return Ok(r);
    }
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
    initialize_runtime(app, &s, config)
}
fn initialize_runtime(
    app: &tauri::AppHandle,
    s: &Arc<AccountRuntime>,
    config: ActivateConfig,
) -> Result<()> {
    let global = app.state::<AppState>();
    let _lock = s
        .lifecycle
        .try_lock()
        .map_err(|_| "Account operation in progress")?;
    if s.logout_progress.lock().unwrap().confirmed {
        return Err("此账号已确认注销，请先重试完成本地清理".into());
    }
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
        create_client(app, s)?;
    }
    s.user_disconnected.store(false, Ordering::SeqCst);
    s.connection.lock().unwrap().user_disconnected = false;
    s.connecting.store(true, Ordering::SeqCst);
    set_runtime_state(app, s, "Initializing", "正在初始化此账号");
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
            if store.is_pending(&config.profile_id) {
                return Ok(());
            }
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
        set_runtime_state(app, s, "Error", e);
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
    let ids = with_store(&global, |store| Ok(store.runtime_profile_ids()))?;
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
async fn disconnect(
    app: tauri::AppHandle,
    profile_id: String,
    close_confirmation: Option<String>,
) -> Result<()> {
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
        if s.logout_progress.lock().unwrap().confirmed {
            return Err("此账号正在完成注销，请在账号设置中重试清理".into());
        }
        approve_and_emit(&app, std::slice::from_ref(&s), close_confirmation)?;
        close_session(&app, &s, false)?;
        s.user_disconnected.store(true, Ordering::SeqCst);
        s.connection.lock().unwrap().user_disconnected = true;
        let _ = app.emit("connection", s.connection.lock().unwrap().clone());
        Ok(())
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
        let g = s.gateway.lock().unwrap().clone();
        let mut progress = s.logout_progress.lock().unwrap();
        if !progress.confirmed
            && !g.as_ref().is_some_and(|g| g.logout_in_progress())
            && s.connection.lock().unwrap().auth_state != "authorizationStateReady"
        {
            return Err("请先恢复此账号连接，再注销账号".into());
        }
        can_transition(&s)?;
        s.closing.store(true, Ordering::SeqCst);
        clear_session(&s);
        set_runtime_state(&app, &s, "Closing", "正在注销 Telegram 授权");
        let outcome = progress.execute(
            g.as_ref()
                .map(|g| g.as_ref() as &dyn crate::gateway::SessionLifecycle),
            || with_store(&global, |store| store.mark_logout_completed(&profile_id)),
            || {
                finish_close(&s, Ok(()))?;
                with_store(&global, |store| {
                    store.cleanup_logged_out_session(&profile_id)?;
                    store.remove(&profile_id)
                })
            },
        );
        if let Err(error) = outcome {
            if progress.confirmed {
                set_runtime_state(
                    &app,
                    &s,
                    "LogoutCleanupRequired",
                    &format!("注销尚未完成，请重试：{error}"),
                );
            } else {
                if error.starts_with("TDLib error ") {
                    s.closing.store(false, Ordering::SeqCst);
                }
                set_runtime_state(
                    &app,
                    &s,
                    "Error",
                    &format!("Telegram 未确认注销，账号登记仍保留：{error}"),
                );
            }
            return Err(error);
        }
        global.manager.accounts.lock().unwrap().remove(&profile_id);
        let _ = app.emit("accounts-changed", ());
        Ok(())
    })
    .await
}
#[tauri::command]
async fn chats(app: tauri::AppHandle, profile_id: String, search: String, scope: Option<ChatListScope>) -> Result<Vec<Chat>> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        let s = account(&global, &profile_id)?;
        let _lock = s
            .lifecycle
            .lock()
            .map_err(|_| "Account lock poisoned")?;
        ChatService::list(gateway(&s)?.as_ref(), &search, scope.unwrap_or_default())
    })
    .await
}
#[tauri::command]
async fn photo_thumbnail(
    app: tauri::AppHandle,
    profile_id: String,
    file_id: Option<i32>,
    full: Option<bool>,
    window: tauri::WebviewWindow,
) -> Result<Option<String>> {
    if full.unwrap_or(false) && window.label()!="main" {return Err("Full media restricted to workspace".into());}
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global.lifecycle.try_read().map_err(|_| "Application transition in progress")?;
        let runtime = account(&global, &profile_id)?;
        let _lock = runtime.lifecycle.lock().map_err(|_| "Account lock poisoned")?;
        // TDLib file IDs are process-scoped. Never accept a path or arbitrary URL from WebView.
        let g = gateway(&runtime)?;
        let mut fallback: Option<String> = None;
        let id = match file_id {
            Some(id) => id,
            None => {
                let me = g.request(json!({"@type":"getMe"}), &AtomicBool::new(false))?;
                fallback = me["profile_photo"]["minithumbnail"]["data"].as_str()
                    .filter(|data|data.len()<16_384 && data.chars().all(|c|c.is_ascii_alphanumeric()||matches!(c,'+'|'/'|'=')))
                    .map(|data|format!("data:image/jpeg;base64,{data}"));
                let Some(id) = me["profile_photo"]["small"]["id"].as_i64().or_else(||me["profile_photo"]["big"]["id"].as_i64()).and_then(|id| i32::try_from(id).ok()) else {return Ok(fallback)};

                id
            }
        };
        // File content is only read from this account's TDLib-managed files directory.
        let root = with_store(&global, |store| Ok(store.session_root(&profile_id)?))?;
        match ChatService::media_data(g.as_ref(), i64::from(id), &root.join("files"), full.unwrap_or(false)) {
            Ok(Some(photo)) => Ok(Some(photo)),
            // A small TDLib minithumbnail is preferable to an empty account card
            // while a larger photo is temporarily unavailable.
            Ok(None) | Err(_) if fallback.is_some() => Ok(fallback),
            other => other,
        }
    }).await
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
async fn start_query(
    app: tauri::AppHandle,
    profile_id: String,
    spec: QuerySpec,
    run_id: String,
) -> Result<()> {
    // Validation, session-lock arbitration and replacement of a potentially
    // large previous ranking must not run on the Tauri/WebView UI thread.
    background(move || {
        let (g,epoch,cancel,previous)={
            let global=app.state::<AppState>();
            let _gate=global.lifecycle.try_read()
                .map_err(|_| "Application transition in progress")?;
            let s=account(&global,&profile_id)?;
            let _lock=s.lifecycle.try_lock()
                .map_err(|_| "Account operation in progress")?;
            spec.validate()?;
            if s.closing.load(Ordering::SeqCst) || s.connecting.load(Ordering::SeqCst){
                return Err("Session transition in progress".into());
            }
            if run_id.is_empty()||run_id.len()>100{
                return Err("Invalid run ID".into());
            }
            let g=gateway(&s)?;
            let epoch=s.epoch.load(Ordering::SeqCst);
            let cancel=Arc::new(AtomicBool::new(false));
            if let Some((_,old))=s.active.lock().unwrap()
                .replace((run_id.clone(),cancel.clone())){
                old.store(true,Ordering::Relaxed);
            }
            let previous=s.results.lock().unwrap().take();
            (g,epoch,cancel,previous)
        };
        // Do not hold lifecycle / active / results mutexes while dropping Post data.
        drop(previous);
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
                    stop_reason:Some(core::StopReason::GatewayFailure),
                    scope:spec.scope.clone(),
                    coverage:core::ScanCoverage::default(),
                    stats:core::ScanStats::default(),
                    scanned: 0,
                    pages: 0,
                    exact: false,
                    detail: e,
                    posts: Vec::new(),
                }),
            }
        });
        Ok(())
    }).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_operations_reject_missing_targets_without_using_another_runtime() {
        let global = AppState::default();
        let a = test_account("A", Duration::ZERO);
        global
            .manager
            .accounts
            .lock()
            .unwrap()
            .insert("A".into(), a.clone());
        for id in ["", "B"] {
            assert!(account(&global, id).is_err());
        }
        assert!(Arc::ptr_eq(&account(&global, "A").unwrap(), &a));
        assert_eq!(a.pending.lock().unwrap().len(), 1);
        assert!(!a.user_disconnected.load(Ordering::SeqCst));
    }
    fn test_account(id: &str, age: Duration) -> Arc<AccountRuntime> {
        let s = Arc::new(AccountRuntime {
            profile_id: id.into(),
            ..AccountRuntime::default()
        });
        s.pending.lock().unwrap().insert(
            "10:20".into(),
            PendingSend {
                started_at: Instant::now() - age,
            },
        );
        s
    }
    #[test]
    fn pending_close_requires_timeout_and_single_use_scoped_confirmation() {
        let global = AppState::default();
        let a = test_account("A", Duration::ZERO);
        let b = test_account("B", Duration::from_secs(31));
        let challenge = pending_challenge(&global, std::slice::from_ref(&a)).unwrap();
        assert_eq!(challenge.pending_count, 1);
        assert!(challenge.retry_after_ms > 0);
        assert!(challenge.token.is_none());
        assert!(authorize_close(&global, std::slice::from_ref(&a), None).is_err());
        a.pending
            .lock()
            .unwrap()
            .get_mut("10:20")
            .unwrap()
            .started_at = Instant::now() - Duration::from_secs(31);
        let challenge = pending_challenge(&global, std::slice::from_ref(&a)).unwrap();
        let token = challenge.token.unwrap();
        assert!(authorize_close(&global, std::slice::from_ref(&b), Some(token)).is_err());
        assert_eq!(a.pending.lock().unwrap().len(), 1);
        assert_eq!(b.pending.lock().unwrap().len(), 1);
        let token = pending_challenge(&global, std::slice::from_ref(&a))
            .unwrap()
            .token
            .unwrap();
        let events =
            authorize_close(&global, std::slice::from_ref(&a), Some(token.clone())).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, "Unknown");
        assert_eq!(events[0].profile_id, "A");
        assert!(a.pending.lock().unwrap().is_empty());
        assert_eq!(b.pending.lock().unwrap().len(), 1);
        assert_eq!(
            a.sent_updates
                .lock()
                .unwrap()
                .get("10:20")
                .map(String::as_str),
            Some("Unknown")
        );
        assert_eq!(a.unknown_sends.lock().unwrap().len(), 1);
        assert!(authorize_close(&global, std::slice::from_ref(&a), Some(token)).is_err());
        finish_close(&a, Ok(())).unwrap();
        assert_eq!(a.unknown_sends.lock().unwrap().len(), 1);
    }
    #[test]
    fn new_pending_or_new_generation_invalidates_confirmation_without_partial_resolution() {
        let global = AppState::default();
        let a = test_account("A", Duration::from_secs(31));
        let b = Arc::new(AccountRuntime {
            profile_id: "B".into(),
            ..AccountRuntime::default()
        });
        let runtimes = vec![a.clone(), b.clone()];
        let token = pending_challenge(&global, &runtimes).unwrap().token;
        b.pending.lock().unwrap().insert(
            "30:40".into(),
            PendingSend {
                started_at: Instant::now() - Duration::from_secs(31),
            },
        );
        assert!(authorize_close(&global, &runtimes, token).is_err());
        assert_eq!(a.pending.lock().unwrap().len(), 1);
        assert!(a.unknown_sends.lock().unwrap().is_empty());
        assert_eq!(b.pending.lock().unwrap().len(), 1);
        let token = pending_challenge(&global, &runtimes).unwrap().token;
        a.epoch.fetch_add(1, Ordering::SeqCst);
        assert!(authorize_close(&global, &runtimes, token).is_err());
        assert_eq!(b.pending.lock().unwrap().len(), 1);
        assert!(b.unknown_sends.lock().unwrap().is_empty());
        let token = pending_challenge(&global, &runtimes).unwrap().token;
        a.forwarding.store(true, Ordering::SeqCst);
        assert!(authorize_close(&global, &runtimes, token).is_err());
        assert_eq!(a.pending.lock().unwrap().len(), 1);
    }
    #[test]
    fn expired_confirmation_is_rejected_and_definitive_receipts_do_not_need_override() {
        let global = AppState::default();
        let a = test_account("A", Duration::from_secs(31));
        let token = pending_challenge(&global, std::slice::from_ref(&a))
            .unwrap()
            .token
            .unwrap();
        global
            .close_approvals
            .lock()
            .unwrap()
            .get_mut(&token)
            .unwrap()
            .created_at = Instant::now() - Duration::from_secs(301);
        assert!(authorize_close(&global, std::slice::from_ref(&a), Some(token)).is_err());
        let token = pending_challenge(&global, std::slice::from_ref(&a))
            .unwrap()
            .token;
        a.pending.lock().unwrap().clear();
        assert!(
            authorize_close(&global, std::slice::from_ref(&a), token)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn code_diagnostics_survives_network_updates_and_clears_on_auth_change() {
        let runtime = AccountRuntime {
            profile_id: "test-login".into(),
            ..AccountRuntime::default()
        };
        let state = json!({
            "@type": "updateAuthorizationState",
            "authorization_state": {
                "@type": "authorizationStateWaitCode",
                "code_info": {
                    "@type": "authenticationCodeInfo",
                    "phone_number": "+8000111222333",
                    "type": {"@type": "authenticationCodeTypeTelegramMessage", "length": 5},
                    "next_type": null,
                    "timeout": 0
                }
            }
        });
        let (connection, ready) = apply_connection(&runtime, 0, &state).unwrap();
        assert!(!ready);
        assert_eq!(connection.auth_state, "authorizationStateWaitCode");
        assert_eq!(connection.code_diagnostics.as_ref().unwrap().length, Some(5));
        let serialized = serde_json::to_string(&connection).unwrap();
        assert!(!serialized.contains("+8000111222333"));
        assert!(serialized.contains("authenticationCodeTypeTelegramMessage"));

        // A connectionState change must NOT overwrite code_info diagnostics.
        let (connected, _) = apply_connection(
            &runtime,
            0,
            &json!({"@type":"updateConnectionState",
                "state":{"@type":"connectionStateReady"}}),
        )
        .unwrap();
        assert_eq!(connected.code_diagnostics, connection.code_diagnostics);

        let (password, _) = apply_connection(
            &runtime,
            0,
            &json!({"@type":"updateAuthorizationState",
                "authorization_state":{"@type":"authorizationStateWaitPassword"}}),
        )
        .unwrap();
        assert!(password.code_diagnostics.is_none());
        runtime.epoch.store(1, Ordering::SeqCst);
        assert!(apply_connection(&runtime, 0, &state).is_none());
    }

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
            stop_reason:Some(core::StopReason::PostLimitReached),
            scope:core::Scope::Recent{count:50},
            coverage:core::ScanCoverage {scope_complete:true,..Default::default()},
            stats:core::ScanStats::default(),
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
            .insert("1:2".into(), PendingSend::default());
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
            .insert("1:2".into(), PendingSend::default());
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
            .insert("1:2".into(), PendingSend::default());
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
    fn incomplete_reset_refuses_new_account_work_but_keeps_close_inventory() {
        let global = AppState::default();
        let runtime = Arc::new(AccountRuntime {
            profile_id: "p-a".into(),
            ..AccountRuntime::default()
        });
        global
            .manager
            .accounts
            .lock()
            .unwrap()
            .insert("p-a".into(), runtime);
        global.reset_cleanup_pending.store(true, Ordering::SeqCst);
        assert!(ensure_running(&global).is_err());
        assert!(account(&global, "p-a").is_err());
        assert_eq!(accounts(&global).len(), 1); // Reset retry / safe exit can still close every client.
    }
    #[test]
    fn old_run_cannot_replace_new_results_or_forward_stale_selection() {
        let s = AccountRuntime::default();
        *s.active.lock().unwrap() = Some(("new".into(), Arc::new(AtomicBool::new(false))));
        let old = ScanResult {
            run_id: "old".into(),
            status: core::ScanStatus::Complete,
            stop_reason:Some(core::StopReason::PostLimitReached),
            scope:core::Scope::Recent{count:50},
            coverage:core::ScanCoverage {scope_complete:true,..Default::default()},
            stats:core::ScanStats::default(),
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
            status: core::ScanStatus::Running,
            ..old.clone()
        };
        assert!(retain_result(&s, &current, 0));
        assert!(selection(&s, "new", &[], false).is_err());
        let current = ScanResult {
            status: core::ScanStatus::Complete,
            ..current
        };
        assert!(retain_result(&s, &current, 0));
        assert!(!retain_result(&s, &old, 0));
        assert_eq!(s.results.lock().unwrap().as_ref().unwrap().run_id, "new");
        assert!(selection(&s, "old", &[], false).is_err());
        assert!(selection(&s, "new", &["missing-post".into()], false).is_err());
        s.epoch.store(1, Ordering::SeqCst);
        assert!(!retain_result(&s, &current, 0));
        assert!(retain_result(&s, &current, 1));
    }
    #[test]
    fn release_result_frees_only_the_matching_run_and_rejects_late_progress() {
        let s=AccountRuntime::default();
        let cancel=Arc::new(AtomicBool::new(false));
        *s.active.lock().unwrap()=Some(("old-run".into(),cancel.clone()));
        let old=ScanResult {
            run_id:"old-run".into(),status:core::ScanStatus::Complete,
            stop_reason:Some(core::StopReason::PostLimitReached),
            scope:core::Scope::Recent{count:1},
            coverage:core::ScanCoverage{scope_complete:true,..Default::default()},
            stats:core::ScanStats::default(),
            scanned:1,pages:1,exact:true,detail:String::new(),posts:vec![],
        };
        assert!(retain_result(&s,&old,0));
        release_result(&s,"other-run");
        assert!(!cancel.load(Ordering::Relaxed));
        assert_eq!(s.results.lock().unwrap().as_ref().unwrap().run_id,"old-run");
        release_result(&s,"old-run");
        assert!(cancel.load(Ordering::Relaxed));
        assert!(s.active.lock().unwrap().is_none());
        assert!(s.results.lock().unwrap().is_none());
        assert!(!retain_result(&s,&old,0));
        let newer=ScanResult{run_id:"new-run".into(),..old};
        *s.active.lock().unwrap()=Some(("new-run".into(),Arc::new(AtomicBool::new(false))));
        assert!(retain_result(&s,&newer,0));
        release_result(&s,"old-run");
        assert_eq!(s.results.lock().unwrap().as_ref().unwrap().run_id,"new-run");
    }
}
#[tauri::command]
async fn release_query(app: tauri::AppHandle,profile_id:String,run_id:String)->Result<()> {
    background(move || {
        let global=app.state::<AppState>();
        let s=account(&global,&profile_id)?;
        if !run_id.is_empty(){release_result(&s,&run_id);}
        Ok(())
    }).await
}
#[tauri::command]
async fn cancel_query(app: tauri::AppHandle,profile_id:String,run_id:String)->Result<()> {
    background(move || {
        let global=app.state::<AppState>();
        let s=account(&global,&profile_id)?;
        if let Some((id,c))=s.active.lock().unwrap().as_ref()
            && id==&run_id
        {
            c.store(true,Ordering::Relaxed);
        }
        Ok(())
    }).await
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
    allow_incomplete: Option<bool>,
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
        let keys = selection(&s, &run_id, &post_keys, allow_incomplete.unwrap_or(false))?;
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
    allow_incomplete: Option<bool>,
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
            let keys = selection(&s, &run_id, &post_keys, allow_incomplete.unwrap_or(false))?;
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
                    pending
                        .entry(format!("{target}:{id}"))
                        .or_insert_with(PendingSend::default);
                }
            }
            Ok(report)
        })();
        s.forwarding.store(false, Ordering::SeqCst);
        r
    })
    .await
}
fn close_all(app: &tauri::AppHandle, confirmation: Option<String>) -> Result<()> {
    let global = app.state::<AppState>();
    let runtimes = accounts(&global);
    approve_and_emit(app, &runtimes, confirmation)?;
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
        let global = &global;
        let handles: Vec<_> = runtimes
            .iter()
            .map(|s| {
                scope.spawn(move || {
                    let g = s.gateway.lock().unwrap().clone();
                    let outcome = g.map_or(Ok(()), |g| {
                        let mut progress = s.logout_progress.lock().unwrap();
                        if g.logout_in_progress() || progress.confirmed {
                            progress.observe_completion(Some(g.as_ref()), || {
                                with_store(global, |store| {
                                    store.mark_logout_completed(&s.profile_id)
                                })
                            })
                        } else {
                            g.wait_closed()
                        }
                    });
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
#[tauri::command]
fn exit_application(app: tauri::AppHandle, close_confirmation: Option<String>) {
    begin_safe_exit(&app, close_confirmation);
}
fn begin_safe_exit(app: &tauri::AppHandle, close_confirmation: Option<String>) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let s = handle.state::<AppState>();
        if s.exiting.swap(true, Ordering::SeqCst) {
            return;
        }
        let _lock = s.lifecycle.write().unwrap();
        match close_all(&handle, close_confirmation) {
            Ok(()) => handle.exit(0),
            Err(e) => {
                s.exiting.store(false, Ordering::SeqCst);
                match pending_challenge(&s, &accounts(&s)) {
                    Ok(challenge) if challenge.pending_count > 0 => {
                        if let Some(main) = handle.get_webview_window("main") {
                            let _ = main.emit("close-confirmation-required", challenge);
                        }
                    }
                    _ => {
                        let _ = handle.emit("app-error", e);
                    }
                }
            }
        }
    });
}
#[cfg(debug_assertions)]
struct DebugResetBackend<'a> {
    app: &'a tauri::AppHandle,
    root: std::path::PathBuf,
}
#[cfg(debug_assertions)]
fn reset_authorization(
    app: &tauri::AppHandle,
    s: &Arc<AccountRuntime>,
    linked: bool,
) -> Result<bool> {
    use crate::gateway::SessionLifecycle;
    let g = s.gateway.lock().unwrap().clone();
    if s.logout_progress.lock().unwrap().confirmed
        || g.as_ref().is_some_and(|g| g.logout_in_progress())
    {
        return Ok(true); // Continue observing the original logout, never restore/race it.
    }
    if let Some(g) = &g {
        let state = s.connection.lock().unwrap().auth_state.clone();
        if !g.is_closed() && state == "authorizationStateReady" {
            return Ok(true);
        }
        if !g.is_closed()
            && matches!(
                state.as_str(),
                "authorizationStateClosing" | "authorizationStateLoggingOut"
            )
        {
            return Err("客户端生命周期尚未完成；请安全关闭后重试，不能把 close 当作注销".into());
        }
        if !g.is_closed() && state != "authorizationStateWaitTdlibParameters" {
            if !linked
                && matches!(
                    state.as_str(),
                    "authorizationStateWaitPhoneNumber"
                        | "authorizationStateWaitCode"
                        | "authorizationStateWaitPassword"
                        | "authorizationStateWaitEmailAddress"
                        | "authorizationStateWaitEmailCode"
                        | "authorizationStateWaitRegistration"
                        | "authorizationStateWaitOtherDeviceConfirmation"
                )
            {
                return Ok(false);
            }
            return Err("请先恢复账号授权和连接，再重试完整重置".into());
        }
    }
    let global = app.state::<AppState>();
    let config = with_store(&global, |store| {
        let root = store.session_root(&s.profile_id)?;
        crate::managed_data::validate_owned(&store.root, &root)?;
        if !root.join("db").is_dir() {
            if linked {
                return Err("原有 Session 数据库缺失；无法确认 Telegram 注销，请恢复后重试".into());
            }
            return Ok(None);
        }
        let passphrase = s.unlocked_passphrase.lock().unwrap().clone();
        let credentials = store.account_credentials(&s.profile_id, passphrase.clone())?;
        if credentials.passphrase.is_none() {
            return Err("缺少原有数据库口令；请在账号设置或登录恢复中解锁此 Session 后重试".into());
        }
        Ok(Some(ActivateConfig {
            profile_id: s.profile_id.clone(),
            passphrase,
            remember_database: credentials.remember_database,
        }))
    })?;
    let Some(config) = config else {
        return Ok(false);
    };
    s.closing.store(false, Ordering::SeqCst);
    initialize_runtime(app, s, config)?;
    let g = s
        .gateway
        .lock()
        .unwrap()
        .clone()
        .ok_or("Client unavailable")?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let auth = g.request(
            json!({"@type":"getAuthorizationState"}),
            &AtomicBool::new(false),
        )?;
        match auth["@type"].as_str() {
            Some("authorizationStateReady") => return Ok(true),
            Some("authorizationStateWaitTdlibParameters") if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Some(
                "authorizationStateWaitPhoneNumber"
                | "authorizationStateWaitCode"
                | "authorizationStateWaitPassword"
                | "authorizationStateWaitEmailAddress"
                | "authorizationStateWaitEmailCode"
                | "authorizationStateWaitRegistration"
                | "authorizationStateWaitOtherDeviceConfirmation",
            ) if !linked => return Ok(false),
            _ => {
                return Err(
                    "原有 Session 未恢复到可注销状态；请恢复授权、数据库口令或网络后重试".into(),
                );
            }
        }
    }
}
#[cfg(debug_assertions)]
impl crate::reset::ResetBackend for DebugResetBackend<'_> {
    fn inventory(&self) -> Result<Vec<crate::reset::Participant>> {
        let global = self.app.state::<AppState>();
        with_store(&global, |store| {
            store
                .runtime_profile_ids()
                .iter()
                .map(|id| {
                    let p = store.profile(id)?;
                    Ok(crate::reset::Participant {
                        profile_id: id.clone(),
                        display_name: p.display_name.clone(),
                        linked: !store.is_pending(id),
                    })
                })
                .collect()
        })
    }
    fn end_participant(&self, p: &crate::reset::Participant) -> Result<()> {
        let global = self.app.state::<AppState>();
        // Inventory comes from AccountRegistry, including actively disconnected accounts.
        let s = create_runtime(self.app, &p.profile_id)?;
        let completed = {
            let progress = s.logout_progress.lock().unwrap();
            progress.closed && progress.recorded
        };
        if completed || (!p.linked && s.reset_pending_closed.load(Ordering::SeqCst)) {
            return Ok(());
        }
        let authorized = reset_authorization(self.app, &s, p.linked)?;
        let _lock = s.lifecycle.lock().unwrap();
        can_transition(&s)?;
        clear_session(&s);
        if authorized {
            s.closing.store(true, Ordering::SeqCst);
            set_runtime_state(
                self.app,
                &s,
                "Closing",
                "完整重置：正在注销 Telegram 授权，等待注销完成",
            );
            let g = s.gateway.lock().unwrap().clone();
            let outcome = s.logout_progress.lock().unwrap().execute(
                g.as_ref()
                    .map(|g| g.as_ref() as &dyn crate::gateway::SessionLifecycle),
                || with_store(&global, |store| store.mark_logout_completed(&p.profile_id)),
                || {
                    finish_close(&s, Ok(()))?;
                    Ok(())
                },
            );
            match outcome {
                Ok(()) => {
                    *s.unlocked_passphrase.lock().unwrap() = None;
                    set_runtime_state(
                        self.app,
                        &s,
                        "LogoutCleanupRequired",
                        "已完成注销；等待全部账号注销成功后统一清理",
                    );
                    Ok(())
                }
                Err(error) => {
                    set_runtime_state(self.app, &s, "Error", &format!("完整重置未完成：{error}"));
                    Err(error)
                }
            }
        } else {
            close_session(self.app, &s, false)?;
            s.reset_pending_closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    fn delete_local_data(&self) -> Result<()> {
        let global = self.app.state::<AppState>();
        // No failed or still-open client may reach the deletion phase.
        if accounts(&global).iter().any(|r| {
            r.gateway
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|g| !g.is_closed())
        }) {
            return Err("仍有未关闭的 TDLib 数据库；完整重置未完成".into());
        }
        global.reset_cleanup_pending.store(true, Ordering::SeqCst);
        crate::managed_data::delete_owned(&self.root, &self.root)
    }
}
#[cfg(debug_assertions)]
#[tauri::command]
async fn debug_reset(
    app: tauri::AppHandle,
    phrase: String,
    close_confirmation: Option<String>,
) -> Result<crate::reset::ResetReport> {
    background(move || {
        if phrase != "RESET TMSGPICKER" {
            return Err("请输入确认短语 RESET TMSGPICKER".into());
        }
        let global = app.state::<AppState>();
        if global.exiting.swap(true, Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        // Refuse new work before waiting for ongoing submissions. Receivers remain active.
        for r in accounts(&global) {
            if let Some((_, cancel)) = r.active.lock().unwrap().as_ref() {
                cancel.store(true, Ordering::SeqCst);
            }
        }
        let _gate = global.lifecycle.write().unwrap();
        let outcome = (|| {
            let root = with_store(&global, |store| Ok(store.root.clone()))?;
            let expected = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .ok_or("LOCALAPPDATA unavailable")?
                .join("com.funare.tmsgpicker");
            if root != expected {
                return Err("Debug reset ownership check failed".into());
            }
            crate::managed_data::validate_owned(&expected, &root)?;
            approve_and_emit(&app, &accounts(&global), close_confirmation)?;
            crate::reset::execute(&DebugResetBackend { app: &app, root })
        })();
        match outcome {
            Ok(report) if report.completed => {
                global.pending_login.lock().unwrap().take();
                global.manager.accounts.lock().unwrap().clear();
                app.exit(0);
                Ok(report)
            }
            Ok(report) => {
                global.exiting.store(false, Ordering::SeqCst);
                let _ = app.emit("accounts-changed", ());
                Ok(report)
            }
            Err(error) => {
                global.exiting.store(false, Ordering::SeqCst);
                Err(error)
            }
        }
    })
    .await
}
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(main) = app.get_webview_window("main") {
                window_theme::apply(&main);
                if let Ok(appearance) = with_store(&app.state::<AppState>(), |store| {
                    Ok(store.settings().appearance)
                }) {
                    window_theme::apply_appearance(&main, &appearance);
                }
            }
            // Register all profiles during setup; initialize in independent workers.
            if let Err(e) = start_accounts(&handle) {
                let _ = handle.emit("app-error", e);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "settings" && matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                // Do not reopen storage while shutdown or complete Debug reset is removing it.
                if state.exiting.load(Ordering::SeqCst)
                    || state.reset_cleanup_pending.load(Ordering::SeqCst)
                {
                    return;
                }
                let snapshot = {
                    let mut session = state.appearance_session.lock().unwrap();
                    session.cancel();
                    with_store(&state, |store| Ok(session.snapshot(store.settings().appearance)))
                };
                match snapshot {
                    Ok(snapshot) => publish_appearance(app, &snapshot),
                    Err(error) => {
                        let _ = app.emit("app-error", error);
                    }
                }
            }
            if window.label() == "login" && matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = cancel_pending_login(&app) {
                        let _ = app.emit(
                            "app-error",
                            format!("登录窗口已关闭，但临时会话清理未完成：{error}"),
                        );
                    }
                });
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "login" {
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = cancel_pending_login(&app) {
                            let _ = app.emit("login-error", error);
                        }
                    });
                }
                route_window_close(window.label(), || {
                    api.prevent_close();
                    begin_safe_exit(window.app_handle(), None);
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_settings,
            settings_target,
            open_login,
            begin_login,
            login_status,
            login_auth_submit,
            retry_login_identity,
            cancel_login,
            #[cfg(debug_assertions)]
            debug_reset,
            app_settings,
            appearance_state,
            preview_appearance,
            save_appearance,
            save_settings,
            reset_settings,
            clear_secret,
            logout_account,
            connection_status,
            close_guard,
            unknown_sends,
            exit_application,
            account_statuses,
            select_profile,
            connect,
            auth_submit,
            disconnect,
            chats,
            public_chat,
            photo_thumbnail,
            saved_messages,
            start_query,
            cancel_query,
            release_query,
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
                begin_safe_exit(app, None);
            }
        }
    });
}
