//! Debug reset has a global cleanup barrier. All account outcomes remain reviewable.
//! The backend supplies registry identities, never frontend paths.
use crate::gateway::Result;
use serde::Serialize;

pub struct Participant {
    pub profile_id: String,
    pub display_name: String,
    pub linked: bool,
}
#[derive(Serialize)]
pub struct AccountOutcome {
    pub profile_id: String,
    pub display_name: String,
    pub completed: bool,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct ResetReport {
    pub completed: bool,
    pub accounts: Vec<AccountOutcome>,
    pub error: Option<String>,
}
pub trait ResetBackend {
    fn inventory(&self) -> Result<Vec<Participant>>;
    /// Linked accounts must logOut + Closed; authorized pending clients also logOut.
    fn end_participant(&self, participant: &Participant) -> Result<()>;
    fn delete_local_data(&self) -> Result<()>;
}
pub fn execute(backend: &impl ResetBackend) -> Result<ResetReport> {
    let participants = backend.inventory()?;
    let accounts: Vec<_> = participants
        .iter()
        .map(|p| {
            let outcome = backend.end_participant(p);
            AccountOutcome {
                profile_id: p.profile_id.clone(),
                display_name: p.display_name.clone(),
                completed: outcome.is_ok(),
                error: outcome.err(),
            }
        })
        .collect();
    if accounts.iter().any(|a| !a.completed) {
        return Ok(ResetReport {
            completed: false,
            accounts,
            error: Some("完整重置未完成；未进入应用目录的最终清理阶段。请恢复失败账号的数据库口令、授权或网络后重试。已完成的注销不可撤销，完成进度已记录。".into()),
        });
    }
    let cleanup = backend.delete_local_data();
    Ok(ResetReport {
        completed: cleanup.is_ok(),
        accounts,
        error: cleanup.err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gateway::{SessionLifecycle, TelegramGateway},
        services::LogoutProgress,
        settings::Store,
    };
    use serde_json::{Value, json};
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        sync::{Mutex, atomic::AtomicBool},
    };

    struct MockClient {
        id: String,
        calls: std::sync::Arc<Mutex<Vec<String>>>,
        failure: Mutex<Option<String>>,
        close_failure: AtomicBool,
    }
    impl TelegramGateway for MockClient {
        fn request(&self, body: Value, _: &AtomicBool) -> Result<Value> {
            assert_eq!(body["@type"], "logOut");
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}:logOut", self.id));
            if let Some(error) = self.failure.lock().unwrap().clone() {
                Err(error)
            } else {
                Ok(json!({"@type":"ok"}))
            }
        }
        fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> {
            unreachable!()
        }
    }
    impl SessionLifecycle for MockClient {
        fn begin_close(&self) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}:close", self.id));
        }
        fn wait_closed(&self) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}:Closed", self.id));
            if self.close_failure.load(std::sync::atomic::Ordering::SeqCst) {
                Err("synthetic Closed timeout".into())
            } else {
                Ok(())
            }
        }
    }
    struct MockReset {
        root: PathBuf,
        store: Mutex<Store>,
        clients: HashMap<String, MockClient>,
        progress: Mutex<HashMap<String, LogoutProgress>>,
        calls: std::sync::Arc<Mutex<Vec<String>>>,
        pending_authorized: bool,
        cleanup_failure: AtomicBool,
    }
    impl MockReset {
        fn new(authorized: bool) -> Self {
            let root = std::env::temp_dir().join(format!(
                "tmsg-mock-reset-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(root.join("tdlib/db")).unwrap(); // Synthetic Legacy only.
            let mut store = Store::open(root.clone()).unwrap();
            store.identify("legacy", &json!({"id":1})).unwrap();
            let a = store.add("A".into()).unwrap();
            store
                .identify(&a, &json!({"id":2,"first_name":"A"}))
                .unwrap();
            let b = store.add("Disconnected B".into()).unwrap();
            store
                .identify(&b, &json!({"id":3,"first_name":"Disconnected B"}))
                .unwrap();
            store.add("Pending".into()).unwrap();
            let calls = std::sync::Arc::new(Mutex::new(Vec::new()));
            let mut clients = HashMap::new();
            for id in store.runtime_profile_ids() {
                fs::create_dir_all(store.session_root(&id).unwrap().join("db")).unwrap();
                fs::write(
                    store.session_root(&id).unwrap().join("db/synthetic"),
                    b"fixture",
                )
                .unwrap();
                store
                    .set_secret(&format!("db:{id}"), Some("synthetic passphrase".into()))
                    .unwrap();
                clients.insert(
                    id.clone(),
                    MockClient {
                        id,
                        calls: calls.clone(),
                        failure: Mutex::new(None),
                        close_failure: AtomicBool::new(false),
                    },
                );
            }
            store
                .set_secret("api", Some("synthetic hash".into()))
                .unwrap();
            store
                .set_secret("proxy", Some("synthetic proxy password".into()))
                .unwrap();
            fs::create_dir_all(root.join("cache")).unwrap();
            fs::write(root.join("cache/synthetic"), b"fixture").unwrap();
            Self {
                root,
                store: Mutex::new(store),
                clients,
                progress: Mutex::new(HashMap::new()),
                calls,
                pending_authorized: authorized,
                cleanup_failure: AtomicBool::new(false),
            }
        }
        fn named(&self, name: &str) -> String {
            let store = self.store.lock().unwrap();
            store
                .runtime_profile_ids()
                .into_iter()
                .find(|id| store.profile(id).unwrap().display_name == name)
                .unwrap()
        }
    }
    impl ResetBackend for MockReset {
        fn inventory(&self) -> Result<Vec<Participant>> {
            let s = self.store.lock().unwrap();
            s.runtime_profile_ids()
                .into_iter()
                .map(|id| {
                    Ok(Participant {
                        display_name: s.profile(&id)?.display_name.clone(),
                        linked: !s.is_pending(&id),
                        profile_id: id,
                    })
                })
                .collect()
        }
        fn end_participant(&self, p: &Participant) -> Result<()> {
            let mut states = self.progress.lock().unwrap();
            let progress = states.entry(p.profile_id.clone()).or_default();
            if progress.closed && progress.recorded {
                return Ok(());
            }
            let client = &self.clients[&p.profile_id];
            if p.linked || self.pending_authorized {
                if p.display_name == "Disconnected B" {
                    let credentials = self
                        .store
                        .lock()
                        .unwrap()
                        .account_credentials(&p.profile_id, None)?;
                    if credentials.passphrase.is_none() {
                        return Err("missing original database passphrase".into());
                    }
                    self.calls
                        .lock()
                        .unwrap()
                        .push(format!("{}:restore", p.profile_id));
                }
                progress.execute(
                    Some(client),
                    || {
                        self.store
                            .lock()
                            .unwrap()
                            .mark_logout_completed(&p.profile_id)
                    },
                    || Ok(()),
                )
            } else {
                client.begin_close();
                client.wait_closed()
            }
        }
        fn delete_local_data(&self) -> Result<()> {
            self.calls.lock().unwrap().push("delete".into());
            if self
                .cleanup_failure
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err("synthetic deletion denied".into());
            }
            crate::managed_data::delete_owned(&self.root, &self.root)
        }
    }
    impl Drop for MockReset {
        fn drop(&mut self) {
            if self.root.exists() {
                fs::remove_dir_all(&self.root).unwrap();
            }
        }
    }
    #[test]
    fn all_linked_and_disconnected_accounts_logout_before_global_cleanup_and_first_start() {
        let backend = MockReset::new(false);
        let b = backend.named("Disconnected B");
        let report = execute(&backend).unwrap();
        assert!(report.completed);
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.last().unwrap(), "delete");
        assert_eq!(calls.iter().filter(|s| s.ends_with(":logOut")).count(), 3);
        assert!(calls.contains(&format!("{b}:restore")));
        assert_eq!(calls.iter().filter(|s| s.ends_with(":close")).count(), 1); // Unauthorized pending only.
        assert!(!backend.root.exists());
        drop(calls);
        let restarted = Store::open(backend.root.clone()).unwrap();
        let snapshot = restarted.snapshot();
        assert!(snapshot.profiles.is_empty());
        assert!(snapshot.interrupted_profiles.is_empty());
        assert!(!snapshot.has_saved_api_hash && !snapshot.has_saved_proxy_password);
        assert_eq!(snapshot.settings.api_id, None);
        assert!(!backend.root.join("tdlib").exists());
    }
    #[test]
    fn partial_network_failure_keeps_sessions_and_secrets_and_retry_skips_completed_accounts() {
        let backend = MockReset::new(true);
        let b = backend.named("Disconnected B");
        *backend.clients[&b].failure.lock().unwrap() = Some("synthetic network outage".into());
        let report = execute(&backend).unwrap();
        assert!(!report.completed);
        assert!(
            report.accounts.iter().any(
                |a| a.profile_id == b && a.error.as_deref() == Some("synthetic network outage")
            )
        );
        assert!(!backend.calls.lock().unwrap().contains(&"delete".into()));
        let reopened = Store::open(backend.root.clone()).unwrap();
        assert!(!reopened.profile(&b).unwrap().logout_completed);
        assert!(reopened.has(&format!("db:{b}")));
        assert!(
            reopened
                .session_root(&b)
                .unwrap()
                .join("db/synthetic")
                .exists()
        );
        let pending = reopened
            .runtime_profile_ids()
            .into_iter()
            .find(|id| reopened.is_pending(id))
            .unwrap();
        assert!(reopened.profile(&pending).unwrap().logout_completed); // Authorized PendingLogin must logout.
        *backend.clients[&b].failure.lock().unwrap() = None;
        assert!(execute(&backend).unwrap().completed);
        let calls = backend.calls.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|s| **s == format!("{b}:logOut"))
                .count(),
            2
        );
        assert_eq!(calls.iter().filter(|s| s.ends_with(":logOut")).count(), 5);
        assert!(!calls.iter().any(|s| s.ends_with(":close")));
    }
    #[test]
    fn missing_disconnected_passphrase_and_closed_timeout_block_cleanup() {
        let backend = MockReset::new(false);
        let b = backend.named("Disconnected B");
        backend
            .store
            .lock()
            .unwrap()
            .set_secret(&format!("db:{b}"), None)
            .unwrap();
        assert!(!execute(&backend).unwrap().completed);
        assert!(
            !backend
                .calls
                .lock()
                .unwrap()
                .contains(&format!("{b}:logOut"))
        );
        backend
            .store
            .lock()
            .unwrap()
            .set_secret(&format!("db:{b}"), Some("synthetic".into()))
            .unwrap();
        backend.clients[&b]
            .close_failure
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!execute(&backend).unwrap().completed);
        assert!(
            !Store::open(backend.root.clone())
                .unwrap()
                .profile(&b)
                .unwrap()
                .logout_completed
        );
        backend.clients[&b]
            .close_failure
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(execute(&backend).unwrap().completed);
        assert_eq!(
            backend
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|s| **s == format!("{b}:logOut"))
                .count(),
            1
        );
    }
    #[test]
    fn deletion_failure_returns_incomplete_and_retry_does_not_repeat_logout() {
        let backend = MockReset::new(false);
        backend
            .cleanup_failure
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let report = execute(&backend).unwrap();
        assert!(!report.completed);
        assert_eq!(report.error.as_deref(), Some("synthetic deletion denied"));
        assert!(report.accounts.iter().all(|a| a.completed));
        backend
            .cleanup_failure
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(execute(&backend).unwrap().completed);
        assert_eq!(
            backend
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.ends_with(":logOut"))
                .count(),
            3
        );
    }
}
