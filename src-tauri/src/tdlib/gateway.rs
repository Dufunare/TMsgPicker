use super::{ffi, runtime::execute_sync};
use crate::gateway::{Result, TelegramGateway};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

type Response = mpsc::Sender<Result<Value>>;
#[derive(Default)]
pub struct Router {
    pending: Mutex<HashMap<String, Response>>,
}
impl Router {
    fn insert(&self, tag: String, tx: Response) {
        self.pending.lock().unwrap().insert(tag, tx);
    }
    fn remove(&self, tag: &str) {
        self.pending.lock().unwrap().remove(tag);
    }
    fn wait(
        &self,
        tag: &str,
        rx: mpsc::Receiver<Result<Value>>,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        let result = loop {
            if cancel.load(Ordering::Relaxed) {
                break Err("Cancelled".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break Err("TDLib request timed out".into());
            }
            match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("TDLib receiver stopped".into());
                }
                Err(_) => {}
            }
        };
        self.remove(tag);
        result
    }
    fn dispatch(&self, v: Value) -> bool {
        let Some(tag) = v["@extra"].as_str() else {
            return false;
        };
        let Some(tx) = self.pending.lock().unwrap().remove(tag) else {
            return true;
        };
        // Never echo TDLib error text: it can contain account data or request parameters.
        let result = if v["@type"] == "error" {
            Err(format!(
                "TDLib error {} (check permissions, connection, credentials; 429 requires waiting)",
                v["code"].as_i64().unwrap_or(-1)
            ))
        } else {
            Ok(v)
        };
        let _ = tx.send(result);
        true
    }
    fn fail_all(&self) {
        for (_, tx) in self.pending.lock().unwrap().drain() {
            let _ = tx.send(Err("TDLib client closed".into()));
        }
    }
}
static ACTIVE: AtomicBool = AtomicBool::new(false);
struct PendingRequest {
    tag: String,
    router: Arc<Router>,
    receiver: Option<mpsc::Receiver<Result<Value>>>,
    deadline: Instant,
}
impl PendingRequest {
    fn wait(mut self, cancel: &AtomicBool) -> Result<Value> {
        let receiver = self.receiver.take().unwrap();
        self.router.wait(
            &self.tag,
            receiver,
            cancel,
            self.deadline.saturating_duration_since(Instant::now()),
        )
    }
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.router.remove(&self.tag);
    }
}
pub struct TdlibGateway {
    client: i32,
    router: Arc<Router>,
    sequence: AtomicU64,
    closed: Arc<(Mutex<bool>, Condvar)>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}
impl TdlibGateway {
    fn enqueue(&self, mut body: Value) -> Result<PendingRequest> {
        if *self.closed.0.lock().unwrap() {
            return Err("TDLib is closed".into());
        }
        let tag = self.sequence.fetch_add(1, Ordering::Relaxed).to_string();
        body["@extra"] = json!(tag);
        let request = CString::new(body.to_string()).map_err(|_| "Invalid TDLib request")?;
        let (tx, rx) = mpsc::channel();
        self.router.insert(tag.clone(), tx);
        unsafe {
            ffi::td_send(self.client, request.as_ptr());
        }
        Ok(PendingRequest {
            tag,
            router: self.router.clone(),
            receiver: Some(rx),
            deadline: Instant::now() + Duration::from_secs(45),
        })
    }
    pub fn new(on_update: impl Fn(Value) + Send + 'static) -> Result<Arc<Self>> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("A TDLib receiver is already running; close it first".into());
        }
        if let Err(e) =
            execute_sync(&json!({"@type":"setLogVerbosityLevel","new_verbosity_level":0}))
        {
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(e);
        }
        // All execute calls are complete before receive begins. The legacy CLI is a separate process.
        let client = unsafe { ffi::td_create_client_id() };
        let router = Arc::new(Router::default());
        let closed = Arc::new((Mutex::new(false), Condvar::new()));
        let r = router.clone();
        let c = closed.clone();
        let worker = thread::spawn(move || {
            loop {
                let ptr = unsafe { ffi::td_receive(0.1) };
                if ptr.is_null() {
                    continue;
                }
                // Own the response before any subsequent receive on this dedicated thread.
                let owned = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                let Ok(v) = serde_json::from_str::<Value>(&owned) else {
                    continue;
                };
                if v["@client_id"].as_i64() != Some(i64::from(client)) {
                    continue;
                }
                let is_closed = v["@type"] == "updateAuthorizationState"
                    && v["authorization_state"]["@type"] == "authorizationStateClosed";
                if !r.dispatch(v.clone()) {
                    on_update(v);
                }
                if is_closed {
                    r.fail_all();
                    *c.0.lock().unwrap() = true;
                    c.1.notify_all();
                    break;
                }
            }
            ACTIVE.store(false, Ordering::SeqCst);
        });
        Ok(Arc::new(Self {
            client,
            router,
            sequence: AtomicU64::new(1),
            closed,
            worker: Mutex::new(Some(worker)),
        }))
    }
    pub fn shutdown(&self) -> Result<()> {
        if !*self.closed.0.lock().unwrap() {
            let request = CString::new(json!({"@type":"close"}).to_string()).unwrap();
            unsafe {
                ffi::td_send(self.client, request.as_ptr());
            }
        }
        let (guard, _) = self
            .closed
            .1
            .wait_timeout_while(
                self.closed.0.lock().unwrap(),
                Duration::from_secs(20),
                |closed| !*closed,
            )
            .unwrap();
        if !*guard {
            return Err(
                "Timed out waiting for authorizationStateClosed; session kept open for safe retry"
                    .into(),
            );
        }
        drop(guard);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
        Ok(())
    }
}
impl TelegramGateway for TdlibGateway {
    fn request(&self, body: Value, cancel: &AtomicBool) -> Result<Value> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        self.enqueue(body)?.wait(cancel)
    }
    fn request_batch(&self, requests: Vec<Value>, cancel: &AtomicBool) -> Result<Vec<Value>> {
        let mut pending = Vec::new();
        for body in requests {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let method = body["@type"]
                .as_str()
                .unwrap_or("unknown request")
                .to_string();
            pending.push((method, self.enqueue(body)?));
        }
        let mut responses = Vec::new();
        while let Some((method, request)) = pending.pop() {
            responses.push(request.wait(cancel).map_err(|e| format!("{method}: {e}"))?);
        }
        responses.reverse();
        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_and_cancel_cleanup_pending_and_ignore_late_responses() {
        let r = Router::default();
        let (tx, rx) = mpsc::channel();
        r.insert("timeout".into(), tx);
        assert!(
            r.wait(
                "timeout",
                rx,
                &AtomicBool::new(false),
                Duration::from_millis(2)
            )
            .unwrap_err()
            .contains("timed out")
        );
        assert!(r.pending.lock().unwrap().is_empty());
        assert!(r.dispatch(json!({"@extra":"timeout","@type":"ok"})));
        let (tx, rx) = mpsc::channel();
        r.insert("cancel".into(), tx);
        assert_eq!(
            r.wait(
                "cancel",
                rx,
                &AtomicBool::new(true),
                Duration::from_secs(45)
            )
            .unwrap_err(),
            "Cancelled"
        );
        assert!(r.pending.lock().unwrap().is_empty());
    }
    #[test]
    fn native_receiver_concurrent_requests_cancel_close_and_restart() {
        // No parameters, credentials, session directory, or Telegram network are used.
        let g = TdlibGateway::new(|_| {}).unwrap();
        assert!(TdlibGateway::new(|_| {}).is_err());
        let c = AtomicBool::new(false);
        let auth = g
            .request(json!({"@type":"getAuthorizationState"}), &c)
            .unwrap();
        assert_eq!(auth["@type"], "authorizationStateWaitTdlibParameters");
        // Regression: both proxy APIs are queued while parameters are missing.
        // No DB directory or valid API credentials are submitted in this test.
        for proxy in [
            json!({"@type":"disableProxy"}),
            json!({"@type":"addProxy","proxy":{"@type":"proxy","server":"127.0.0.1","port":7897,"type":{"@type":"proxyTypeSocks5","username":"","password":""}},"enable":true}),
        ] {
            let mut waiting = g.enqueue(proxy.clone()).unwrap();
            let response = g.router.wait(
                &waiting.tag,
                waiting.receiver.take().unwrap(),
                &c,
                Duration::from_millis(40),
            );
            assert!(response.unwrap_err().contains("timed out"));
            drop(waiting);
            assert_eq!(
                g.request(json!({"@type":"getAuthorizationState"}), &c)
                    .unwrap()["@type"],
                "authorizationStateWaitTdlibParameters"
            );
            let start = Instant::now();
            let error = g
                .request_batch(
                    vec![proxy, json!({"@type":"setTdlibParameters","api_id":0})],
                    &c,
                )
                .unwrap_err();
            assert!(error.contains("setTdlibParameters"), "{error}");
            assert!(!error.contains("timed out"), "{error}");
            assert!(start.elapsed() < Duration::from_secs(5));
            assert!(g.router.pending.lock().unwrap().is_empty());
        }
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let g = g.clone();
                thread::spawn(move || {
                    g.request(
                        json!({"@type":"getOption","name":"version"}),
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                })
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap()["@type"], "optionValueString");
        }
        assert!(
            g.request(
                json!({"@type":"getAuthorizationState"}),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(g.router.pending.lock().unwrap().is_empty());
        g.shutdown().unwrap();
        assert!(
            g.request(json!({"@type":"getAuthorizationState"}), &c)
                .is_err()
        );
        let restarted = TdlibGateway::new(|_| {}).unwrap();
        restarted
            .request(json!({"@type":"getAuthorizationState"}), &c)
            .unwrap();
        restarted.shutdown().unwrap();
    }
    #[test]
    fn router_parallel_errors_late_responses_and_close() {
        let r = Router::default();
        let (a, ar) = mpsc::channel();
        let (b, br) = mpsc::channel();
        r.insert("a".into(), a);
        r.insert("b".into(), b);
        assert!(r.dispatch(json!({"@extra":"b","@type":"ok"})));
        assert!(br.recv().unwrap().is_ok());
        assert!(r.dispatch(
            json!({"@extra":"a","@type":"error","code":429,"message":"synthetic-secret"})
        ));
        assert!(!ar.recv().unwrap().unwrap_err().contains("synthetic-secret"));
        let (tx, rx) = mpsc::channel();
        r.insert("late".into(), tx);
        r.remove("late");
        assert!(rx.recv().is_err());
        assert!(r.dispatch(json!({"@extra":"late"})));
        let (tx, rx) = mpsc::channel();
        r.insert("close".into(), tx);
        r.fail_all();
        assert!(rx.recv().unwrap().is_err());
        assert!(r.pending.lock().unwrap().is_empty());
    }
}
