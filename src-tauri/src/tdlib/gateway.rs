use super::{ffi, runtime::execute_sync};
use crate::gateway::{Result, SessionLifecycle, TelegramGateway};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
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
    sealed: AtomicBool,
}
impl Router {
    fn insert(&self, tag: String, tx: Response) -> Result<()> {
        let mut pending = self.pending.lock().unwrap();
        if self.sealed.load(Ordering::SeqCst) {
            return Err("TDLib client closed".into());
        }
        pending.insert(tag, tx);
        Ok(())
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
        let mut pending = self.pending.lock().unwrap();
        self.sealed.store(true, Ordering::SeqCst);
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err("TDLib client closed".into()));
        }
    }
}
// The process owns exactly one receive thread, regardless of the number of clients.
type UpdateHandler = Box<dyn Fn(Value) + Send + Sync>;
struct ClientSlot {
    router: Arc<Router>,
    closed: Arc<(Mutex<bool>, Condvar)>,
    on_update: UpdateHandler,
}
struct SharedReceiver {
    clients: Arc<Mutex<HashMap<i32, Arc<ClientSlot>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
static RECEIVER: OnceLock<Mutex<Option<SharedReceiver>>> = OnceLock::new();
#[cfg(test)]
static RECEIVE_LOOPS: AtomicU64 = AtomicU64::new(0);
fn receiver() -> &'static Mutex<Option<SharedReceiver>> {
    RECEIVER.get_or_init(|| Mutex::new(None))
}
impl SharedReceiver {
    fn start() -> Result<Self> {
        execute_sync(&json!({"@type":"setLogVerbosityLevel","new_verbosity_level":0}))?;
        let clients = Arc::new(Mutex::new(HashMap::<i32, Arc<ClientSlot>>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let map = clients.clone();
        let flag = stop.clone();
        let worker = thread::Builder::new()
            .name("tdlib-shared-receiver".into())
            .spawn(move || {
                #[cfg(test)]
                assert_eq!(RECEIVE_LOOPS.fetch_add(1, Ordering::SeqCst), 0);
                while !flag.load(Ordering::SeqCst) {
                    let ptr = unsafe { ffi::td_receive(0.1) };
                    if ptr.is_null() {
                        continue;
                    }
                    // Copy before the next receive/execute on this thread invalidates the pointer.
                    let owned = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                    let Ok(v) = serde_json::from_str::<Value>(&owned) else {
                        continue;
                    };
                    dispatch_client(&map, v);
                }
                #[cfg(test)]
                assert_eq!(RECEIVE_LOOPS.fetch_sub(1, Ordering::SeqCst), 1);
            })
            .map_err(|_| "Cannot start TDLib receiver")?;
        Ok(Self {
            clients,
            stop,
            worker: Some(worker),
        })
    }
}
fn dispatch_client(map: &Mutex<HashMap<i32, Arc<ClientSlot>>>, v: Value) {
    let Some(id) = v["@client_id"]
        .as_i64()
        .and_then(|id| i32::try_from(id).ok())
    else {
        return;
    };
    let slot = map.lock().unwrap().get(&id).cloned();
    let Some(slot) = slot else {
        return;
    }; // Unknown/retired client, including late replies.
    let closed = v["@type"] == "updateAuthorizationState"
        && v["authorization_state"]["@type"] == "authorizationStateClosed";
    if !slot.router.dispatch(v.clone()) {
        (slot.on_update)(v);
    }
    if closed {
        slot.router.fail_all();
        map.lock().unwrap().remove(&id);
        *slot.closed.0.lock().unwrap() = true;
        slot.closed.1.notify_all();
    }
}
fn stop_receiver_if_empty() {
    // Serialize join with client creation, so two receive loops can never overlap.
    let mut shared = receiver().lock().unwrap();
    if shared
        .as_ref()
        .is_some_and(|r| r.clients.lock().unwrap().is_empty())
    {
        let mut r = shared.take().unwrap();
        r.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = r.worker.take() {
            let _ = worker.join();
        }
    }
}
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
        self.router.insert(tag.clone(), tx)?;
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
    pub fn new(on_update: impl Fn(Value) + Send + Sync + 'static) -> Result<Arc<Self>> {
        let mut shared = receiver().lock().unwrap();
        if shared.is_none() {
            *shared = Some(SharedReceiver::start()?);
        }
        let r = shared.as_ref().unwrap();
        // Hold the map lock from creation through registration: no early update can be lost.
        let mut clients = r.clients.lock().unwrap();
        let client = unsafe { ffi::td_create_client_id() };
        if client <= 0 {
            drop(clients);
            drop(shared);
            stop_receiver_if_empty();
            return Err("Cannot create TDLib client".into());
        }
        let router = Arc::new(Router::default());
        let closed = Arc::new((Mutex::new(false), Condvar::new()));
        clients.insert(
            client,
            Arc::new(ClientSlot {
                router: router.clone(),
                closed: closed.clone(),
                on_update: Box::new(on_update),
            }),
        );
        Ok(Arc::new(Self {
            client,
            router,
            sequence: AtomicU64::new(1),
            closed,
        }))
    }
    pub fn is_closed(&self) -> bool {
        *self.closed.0.lock().unwrap()
    }
    pub fn start_authorization_updates(&self) {
        // TDLib begins emitting updates after the first request. No synchronous wait
        // here: a broken client must not delay creation of the next profile's client.
        let request = CString::new(json!({"@type":"getAuthorizationState"}).to_string()).unwrap();
        unsafe {
            ffi::td_send(self.client, request.as_ptr());
        }
    }
    pub fn begin_close(&self) {
        if !self.is_closed() {
            let request = CString::new(json!({"@type":"close"}).to_string()).unwrap();
            unsafe {
                ffi::td_send(self.client, request.as_ptr());
            }
        }
    }
    pub fn shutdown(&self) -> Result<()> {
        self.begin_close();
        self.wait_closed()
    }
    /// logOut closes TDLib itself; wait without submitting close during logout.
    pub fn wait_closed(&self) -> Result<()> {
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
        stop_receiver_if_empty();
        Ok(())
    }
}
impl SessionLifecycle for TdlibGateway {
    fn begin_close(&self) {
        TdlibGateway::begin_close(self);
    }
    fn wait_closed(&self) -> Result<()> {
        TdlibGateway::wait_closed(self)
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
    fn client_routing_same_tags_updates_close_timeout_and_retired_ids_are_isolated() {
        let map = Mutex::new(HashMap::new());
        let events = Arc::new(Mutex::new(Vec::new()));
        let slot = |id| {
            let events = events.clone();
            let slot = Arc::new(ClientSlot {
                router: Arc::new(Router::default()),
                closed: Arc::new((Mutex::new(false), Condvar::new())),
                on_update: Box::new(move |v| events.lock().unwrap().push((id, v))),
            });
            map.lock().unwrap().insert(id, slot.clone());
            slot
        };
        let a = slot(1);
        let b = slot(2);
        let (atx, arx) = mpsc::channel();
        let (btx, brx) = mpsc::channel();
        a.router.insert("same".into(), atx).unwrap();
        b.router.insert("same".into(), btx).unwrap();
        dispatch_client(&map, json!({"@client_id":2,"@extra":"same","@type":"ok"}));
        assert_eq!(brx.recv().unwrap().unwrap()["@client_id"], 2);
        assert!(matches!(arx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        dispatch_client(
            &map,
            json!({"@client_id":1,"@extra":"same","@type":"error","code":401}),
        );
        assert!(arx.recv().unwrap().is_err());
        for number in [1, 2] {
            dispatch_client(
                &map,
                json!({"@client_id":1,"@type":"syntheticUpdate","number":number}),
            );
        }
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .map(|(id, v)| (*id, v["number"].as_i64().unwrap()))
                .collect::<Vec<_>>(),
            vec![(1, 1), (1, 2)]
        );
        let (atx, arx) = mpsc::channel();
        let (btx, brx) = mpsc::channel();
        a.router.insert("timeout".into(), atx).unwrap();
        b.router.insert("pending".into(), btx).unwrap();
        assert!(
            a.router
                .wait(
                    "timeout",
                    arx,
                    &AtomicBool::new(false),
                    Duration::from_millis(2)
                )
                .is_err()
        );
        assert_eq!(b.router.pending.lock().unwrap().len(), 1);
        let (atx, arx) = mpsc::channel();
        a.router.insert("close".into(), atx).unwrap();
        dispatch_client(
            &map,
            json!({"@client_id":1,"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}),
        );
        assert!(arx.recv().unwrap().is_err());
        assert!(*a.closed.0.lock().unwrap());
        assert!(!*b.closed.0.lock().unwrap());
        assert_eq!(b.router.pending.lock().unwrap().len(), 1);
        let restarted = slot(3);
        let (tx, rx) = mpsc::channel();
        restarted.router.insert("same".into(), tx).unwrap();
        let old_count = events.lock().unwrap().len();
        dispatch_client(&map, json!({"@client_id":1,"@extra":"same","@type":"ok"}));
        dispatch_client(&map, json!({"@client_id":1,"@type":"syntheticUpdate"}));
        dispatch_client(&map, json!({"@client_id":999,"@type":"syntheticUpdate"}));
        assert_eq!(events.lock().unwrap().len(), old_count);
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        dispatch_client(&map, json!({"@client_id":3,"@extra":"same","@type":"ok"}));
        assert_eq!(rx.recv().unwrap().unwrap()["@client_id"], 3);
        dispatch_client(
            &map,
            json!({"@client_id":2,"@extra":"pending","@type":"ok"}),
        );
        assert!(brx.recv().unwrap().is_ok());
        let (tx, _) = mpsc::channel();
        assert!(a.router.insert("after-close".into(), tx).is_err());
    }
    #[test]
    fn timeout_and_cancel_cleanup_pending_and_ignore_late_responses() {
        let r = Router::default();
        let (tx, rx) = mpsc::channel();
        r.insert("timeout".into(), tx).unwrap();
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
        r.insert("cancel".into(), tx).unwrap();
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
        let other = TdlibGateway::new(|_| {}).unwrap();
        assert_ne!(g.client, other.client);
        let c = AtomicBool::new(false);
        let auth = g
            .request(json!({"@type":"getAuthorizationState"}), &c)
            .unwrap();
        assert_eq!(auth["@type"], "authorizationStateWaitTdlibParameters");
        assert_eq!(RECEIVE_LOOPS.load(Ordering::SeqCst), 1);
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
        other
            .request(json!({"@type":"getAuthorizationState"}), &c)
            .unwrap();
        other.shutdown().unwrap();
        assert_eq!(RECEIVE_LOOPS.load(Ordering::SeqCst), 0);
        assert!(
            g.request(json!({"@type":"getAuthorizationState"}), &c)
                .is_err()
        );
        let restarted = TdlibGateway::new(|_| {}).unwrap();
        let second = TdlibGateway::new(|_| {}).unwrap();
        restarted
            .request(json!({"@type":"getAuthorizationState"}), &c)
            .unwrap();
        let workers: Vec<_> = [restarted, second]
            .into_iter()
            .map(|g| {
                thread::spawn(move || {
                    g.request(
                        json!({"@type":"getAuthorizationState"}),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                    g.shutdown().unwrap();
                })
            })
            .collect();
        for h in workers {
            h.join().unwrap();
        }
        assert_eq!(RECEIVE_LOOPS.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn router_parallel_errors_late_responses_and_close() {
        let r = Router::default();
        let (a, ar) = mpsc::channel();
        let (b, br) = mpsc::channel();
        r.insert("a".into(), a).unwrap();
        r.insert("b".into(), b).unwrap();
        assert!(r.dispatch(json!({"@extra":"b","@type":"ok"})));
        assert!(br.recv().unwrap().is_ok());
        assert!(r.dispatch(
            json!({"@extra":"a","@type":"error","code":429,"message":"synthetic-secret"})
        ));
        assert!(!ar.recv().unwrap().unwrap_err().contains("synthetic-secret"));
        let (tx, rx) = mpsc::channel();
        r.insert("late".into(), tx).unwrap();
        r.remove("late");
        assert!(rx.recv().is_err());
        assert!(r.dispatch(json!({"@extra":"late"})));
        let (tx, rx) = mpsc::channel();
        r.insert("close".into(), tx).unwrap();
        r.fail_all();
        assert!(rx.recv().unwrap().is_err());
        assert!(r.pending.lock().unwrap().is_empty());
    }
}
