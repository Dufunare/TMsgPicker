//! PendingLogin owns the wizard; AccountRegistry promotion happens only after getMe.
use super::*;

pub(super) fn complete_identity(app: &tauri::AppHandle, s: &Arc<AccountRuntime>, epoch: u64) {
    let global = app.state::<AppState>();
    let _gate = global.lifecycle.read().unwrap();
    let _lock = s.lifecycle.lock().unwrap();
    if !current_epoch(s, epoch)
        || s.closing.load(Ordering::SeqCst)
        || global.exiting.load(Ordering::SeqCst)
    {
        return;
    }
    let outcome: Result<()> = (|| {
        let me = client(s)?.request(json!({"@type":"getMe"}), &AtomicBool::new(false))?;
        let pending = with_store(&global, |store| Ok(store.is_pending(&s.profile_id)))?;
        let remember = global
            .pending_login
            .lock()
            .unwrap()
            .as_ref()
            .filter(|p| p.profile_id == s.profile_id)
            .map(|p| p.remember_database);
        let secret = if pending && remember == Some(true) {
            Some(Some(
                s.unlocked_passphrase
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or("Database initialization incomplete")?,
            ))
        } else if pending && remember == Some(false) {
            Some(None)
        } else {
            None
        };
        let duplicate = with_store(&global, |store| {
            store.complete_login(&s.profile_id, &me, secret)
        })?;
        if duplicate {
            close_session(app, s, false)?;
            return Err(
                "此 Telegram 账号已添加；原账号和 Session 保持完整。请取消本次登录。".into(),
            );
        }
        let wizard = {
            let mut login = global.pending_login.lock().unwrap();
            if login.as_ref().is_some_and(|p| p.profile_id == s.profile_id) {
                login.take().is_some()
            } else {
                false
            }
        };
        let _ = app.emit("accounts-changed", ());
        if pending && let Some(main) = app.get_webview_window("main") {
            let _ = main.emit("login-completed", &s.profile_id);
        }
        if wizard && let Some(window) = app.get_webview_window("login") {
            let _ = window.destroy();
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        if global
            .pending_login
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|p| p.profile_id == s.profile_id)
        {
            let _ = app.emit("login-error", &error);
        }
        let _ = app.emit(
            "account-notice",
            json!({"profile_id":s.profile_id,"detail":error}),
        );
    }
}

#[tauri::command]
pub(super) async fn open_login(app: tauri::AppHandle) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_read()
            .map_err(|_| "Application transition in progress")?;
        ensure_running(&global)?;
        if global.exiting.load(Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        let _window = global.settings_window.lock().unwrap();
        open_auxiliary(&app, "login", "index.html?view=login", "添加 Telegram 账号")
    })
    .await
}
#[derive(Deserialize)]
pub(super) struct LoginConfig {
    phone: String,
    passphrase: String,
    remember_database: bool,
    recovery_profile_id: Option<String>,
}
#[tauri::command]
pub(super) async fn begin_login(app: tauri::AppHandle, config: LoginConfig) -> Result<String> {
    background(move || {
        let global = app.state::<AppState>();
        let _gate = global
            .lifecycle
            .try_write()
            .map_err(|_| "Application transition in progress")?;
        ensure_running(&global)?;
        if global.exiting.load(Ordering::SeqCst) {
            return Err("Application is shutting down".into());
        }
        if global.pending_login.lock().unwrap().is_some() {
            return Err("已有登录流程，请继续或取消".into());
        }
        if config.passphrase.is_empty() {
            return Err("请输入数据库密码；已有 Session 必须使用原密码".into());
        }
        if config.recovery_profile_id.is_none()
            && (!config.phone.starts_with('+') || config.phone.len() < 6)
        {
            return Err("请输入包含国际区号的手机号".into());
        }
        let id = with_store(&global, |store| {
            if !store.api_configured()? {
                return Err("请先在应用设置中补充 Telegram API ID 和 API Hash".into());
            }
            match &config.recovery_profile_id {
                Some(id) if store.is_pending(id) => Ok(id.clone()),
                Some(_) => Err("此记录不属于未完成登录".into()),
                None => store.add("待完成登录".into()),
            }
        })?;
        *global.pending_login.lock().unwrap() = Some(PendingLogin {
            profile_id: id.clone(),
            phone: if config.phone.is_empty() {
                None
            } else {
                Some(config.phone)
            },
            remember_database: config.remember_database,
            newly_created: config.recovery_profile_id.is_none(),
        });
        create_runtime(&app, &id)?;
        // A failed bootstrap keeps the pending owner for safe cancellation/retry.
        initialize_account(
            &app,
            ActivateConfig {
                profile_id: id.clone(),
                passphrase: Some(config.passphrase),
                remember_database: config.remember_database,
            },
        )?;
        Ok(id)
    })
    .await
}
#[tauri::command]
pub(super) fn login_status(s: State<AppState>) -> Result<Option<Connection>> {
    let id = s
        .pending_login
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.profile_id.clone());
    id.map(|id| account(&s, &id).map(|r| r.connection.lock().unwrap().clone()))
        .transpose()
}
#[tauri::command]
pub(super) async fn retry_login_identity(app: tauri::AppHandle) -> Result<()> {
    background(move || {
        let global = app.state::<AppState>();
        let id = global
            .pending_login
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.profile_id.clone())
            .ok_or("No pending login")?;
        let s = account(&global, &id)?;
        if s.connection.lock().unwrap().auth_state != "authorizationStateReady" {
            return Err("Authorization is incomplete".into());
        }
        complete_identity(&app, &s, s.epoch.load(Ordering::SeqCst));
        Ok(())
    })
    .await
}
pub(super) fn cancel_pending_login(app: &tauri::AppHandle) -> Result<()> {
    let global = app.state::<AppState>();
    let _gate = global.lifecycle.read().unwrap();
    if global.exiting.load(Ordering::SeqCst) {
        return Err("Application is shutting down".into());
    }
    let owner = global
        .pending_login
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| (p.profile_id.clone(), p.newly_created));
    if let Some((id, newly_created)) = owner {
        let s = account(&global, &id)?;
        let _lock = s.lifecycle.lock().unwrap();
        // Re-check after waiting: Ready/getMe may have promoted it meanwhile.
        if with_store(&global, |store| Ok(store.is_pending(&id)))? {
            close_session(app, &s, false)?;
            if newly_created {
                with_store(&global, |store| {
                    store.delete_session(&id)?;
                    store.remove(&id)
                })?;
                global.manager.accounts.lock().unwrap().remove(&id);
            }
        }
        let mut login = global.pending_login.lock().unwrap();
        if login.as_ref().is_some_and(|p| p.profile_id == id) {
            login.take();
        }
        let _ = app.emit("accounts-changed", ());
    }
    if let Some(window) = app.get_webview_window("login") {
        window.destroy().map_err(|_| "Cannot close Login window")?;
    }
    Ok(())
}
#[tauri::command]
pub(super) async fn cancel_login(app: tauri::AppHandle) -> Result<()> {
    background(move || cancel_pending_login(&app)).await
}
