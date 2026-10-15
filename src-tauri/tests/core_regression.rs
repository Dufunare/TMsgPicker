use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tmsgpicker_lib::{
    core::*,
    gateway::{Result, SessionLifecycle, TelegramGateway},
    services::{ActionService, AuthService, ChatListScope, ChatService, ConnectConfig, ProxyConfig},
};

struct MockTelegramGateway {
    pages: Mutex<VecDeque<Result<Value>>>,
    requests: Mutex<Vec<Value>>,
    cancel_history: bool,
}
#[test]
fn combined_login_enforces_tdlib_code_then_optional_password_and_handles_rejections() {
    struct LoginMock {
        steps: Mutex<VecDeque<(&'static str, Result<Value>)>>,
        requests: Mutex<Vec<Value>>,
    }
    impl TelegramGateway for LoginMock {
        fn request(&self, request: Value, _: &AtomicBool) -> Result<Value> {
            let (expected, reply) = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected request");
            assert_eq!(request["@type"], expected);
            self.requests.lock().unwrap().push(request);
            reply
        }
        fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> {
            unreachable!()
        }
    }
    let state = |name: &str| Ok(json!({"@type":name}));
    let mock = |steps: Vec<(&'static str, Result<Value>)>| LoginMock {
        steps: Mutex::new(steps.into()),
        requests: Mutex::new(Vec::new()),
    };
    let g = mock(vec![
        ("getAuthorizationState", state("authorizationStateWaitCode")),
        ("checkAuthenticationCode", state("ok")),
        ("getAuthorizationState", state("authorizationStateWaitCode")),
        (
            "getAuthorizationState",
            state("authorizationStateWaitPassword"),
        ),
        ("checkAuthenticationPassword", state("ok")),
    ]);
    AuthService::submit_login(
        &g,
        "authorizationStateWaitCode",
        "synthetic code".into(),
        "synthetic password".into(),
    )
    .unwrap();
    let requests = g.requests.lock().unwrap();
    assert_eq!(requests[1]["code"], "synthetic code");
    assert_eq!(requests[4]["password"], "synthetic password");
    drop(requests);
    let no_2fa = mock(vec![
        ("getAuthorizationState", state("authorizationStateWaitCode")),
        ("checkAuthenticationCode", state("ok")),
        ("getAuthorizationState", state("authorizationStateReady")),
    ]);
    AuthService::submit_login(
        &no_2fa,
        "authorizationStateWaitCode",
        "synthetic code".into(),
        "unused password".into(),
    )
    .unwrap();
    let wrong_code = mock(vec![
        ("getAuthorizationState", state("authorizationStateWaitCode")),
        (
            "checkAuthenticationCode",
            Err("synthetic wrong code".into()),
        ),
    ]);
    assert!(
        AuthService::submit_login(
            &wrong_code,
            "authorizationStateWaitCode",
            "wrong".into(),
            "must not submit".into()
        )
        .is_err()
    );
    let missing_password = mock(vec![
        ("getAuthorizationState", state("authorizationStateWaitCode")),
        ("checkAuthenticationCode", state("ok")),
        (
            "getAuthorizationState",
            state("authorizationStateWaitPassword"),
        ),
    ]);
    AuthService::submit_login(
        &missing_password,
        "authorizationStateWaitCode",
        "synthetic code".into(),
        String::new(),
    )
    .unwrap();
    let wrong_password = mock(vec![
        (
            "getAuthorizationState",
            state("authorizationStateWaitPassword"),
        ),
        (
            "checkAuthenticationPassword",
            Err("synthetic wrong password".into()),
        ),
    ]);
    assert!(
        AuthService::submit_login(
            &wrong_password,
            "authorizationStateWaitPassword",
            String::new(),
            "wrong".into()
        )
        .is_err()
    );
    let retry = mock(vec![
        (
            "getAuthorizationState",
            state("authorizationStateWaitPassword"),
        ),
        ("checkAuthenticationPassword", state("ok")),
    ]);
    AuthService::submit_login(
        &retry,
        "authorizationStateWaitPassword",
        "never resend code".into(),
        "synthetic password".into(),
    )
    .unwrap();
    let stale = mock(vec![(
        "getAuthorizationState",
        state("authorizationStateWaitPassword"),
    )]);
    assert!(
        AuthService::submit_login(
            &stale,
            "authorizationStateWaitCode",
            "must not send".into(),
            "must not send".into()
        )
        .is_err()
    );
}
impl SessionLifecycle for MockTelegramGateway {
    fn begin_close(&self) {
        self.requests.lock().unwrap().push(json!({"@type":"close"}));
    }
    fn wait_closed(&self) -> Result<()> {
        self.requests
            .lock()
            .unwrap()
            .push(json!({"@type":"waitClosed"}));
        Ok(())
    }
}
#[test]
fn normal_close_and_explicit_logout_have_distinct_lifecycles() {
    for (logout, expected) in [(false, "close"), (true, "logOut")] {
        let g = MockTelegramGateway::pages(Vec::new());
        AuthService::end_session(&g, logout).unwrap();
        let calls = g.requests.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|v| v["@type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![expected, "waitClosed"]
        );
    }
}
struct LogoutMock {
    reply: Mutex<Result<Value>>,
    close_failures: Mutex<usize>,
    calls: Mutex<Vec<&'static str>>,
}
impl TelegramGateway for LogoutMock {
    fn request(&self, _: Value, _: &AtomicBool) -> Result<Value> {
        self.calls.lock().unwrap().push("logOut");
        self.reply.lock().unwrap().clone()
    }
    fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> {
        unreachable!()
    }
}
impl SessionLifecycle for LogoutMock {
    fn begin_close(&self) {
        panic!("logout must never append close");
    }
    fn wait_closed(&self) -> Result<()> {
        self.calls.lock().unwrap().push("waitClosed");
        let mut failures = self.close_failures.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            Err("synthetic close timeout".into())
        } else {
            Ok(())
        }
    }
}
#[test]
fn logout_requires_positive_receipt_and_closed_before_cleanup_and_retries_without_relogout() {
    use tmsgpicker_lib::{services::LogoutProgress, settings::Store};
    let root = std::env::temp_dir().join(format!(
        "tmsg-logout-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut store = Store::open(root.clone()).unwrap();
    let a = store.add("A".into()).unwrap();
    let b = store.add("B".into()).unwrap();
    store.identify(&a, &json!({"id":1})).unwrap();
    store.identify(&b, &json!({"id":2})).unwrap();
    std::fs::create_dir_all(store.session_root(&a).unwrap().join("db")).unwrap();
    std::fs::create_dir_all(store.session_root(&b).unwrap().join("db")).unwrap();
    let g = LogoutMock {
        reply: Mutex::new(Err("synthetic rejection".into())),
        close_failures: Mutex::new(1),
        calls: Mutex::new(Vec::new()),
    };
    let mut progress = LogoutProgress::default();
    assert!(
        progress
            .execute(
                Some(&g),
                || Ok(()),
                || panic!("rejection must retain account")
            )
            .is_err()
    );
    assert!(!progress.confirmed);
    assert!(store.profile(&a).is_ok());
    *g.reply.lock().unwrap() = Ok(json!({"@type":"unexpected"}));
    assert!(
        progress
            .execute(
                Some(&g),
                || Ok(()),
                || panic!("unconfirmed logout must retain account")
            )
            .is_err()
    );
    *g.reply.lock().unwrap() = Ok(json!({"@type":"ok"}));
    assert!(
        progress
            .execute(
                Some(&g),
                || store.mark_logout_completed(&a),
                || panic!("close timeout must retain account and data")
            )
            .is_err()
    );
    assert!(progress.confirmed);
    assert!(!progress.closed);
    assert!(
        !Store::open(root.clone())
            .unwrap()
            .profile(&a)
            .unwrap()
            .logout_completed
    );
    assert!(
        progress
            .execute(
                Some(&g),
                || Ok(()),
                || Err("synthetic cleanup failure".into())
            )
            .is_err()
    );
    assert!(progress.closed);
    assert!(store.profile(&a).is_ok());
    assert!(store.session_root(&a).unwrap().exists());
    progress
        .execute(
            None,
            || Ok(()),
            || {
                store.cleanup_logged_out_session(&a)?;
                store.remove(&a)
            },
        )
        .unwrap();
    assert!(store.profile(&a).is_err());
    assert!(store.session_root(&b).unwrap().join("db").exists());
    assert_eq!(
        store.snapshot().primary_profile_id.as_deref(),
        Some(b.as_str())
    );
    assert_eq!(
        *g.calls.lock().unwrap(),
        vec!["logOut", "logOut", "logOut", "waitClosed", "waitClosed"]
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn timed_out_logout_is_observed_without_resubmission_or_close_and_unverified_closed_is_rejected() {
    use tmsgpicker_lib::services::LogoutProgress;
    struct DelayedLogout {
        submitted: AtomicBool,
        acknowledgement: AtomicBool,
        fail_wait: AtomicBool,
        requests: Mutex<usize>,
    }
    impl TelegramGateway for DelayedLogout {
        fn request(&self, request: Value, _: &AtomicBool) -> Result<Value> {
            assert_eq!(request["@type"], "logOut");
            *self.requests.lock().unwrap() += 1;
            self.submitted.store(true, Ordering::SeqCst);
            Err("synthetic request timeout".into())
        }
        fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> {
            unreachable!()
        }
    }
    impl SessionLifecycle for DelayedLogout {
        fn begin_close(&self) {
            panic!("close must not replace logOut");
        }
        fn wait_closed(&self) -> Result<()> {
            if self.fail_wait.swap(false, Ordering::SeqCst) {
                Err("synthetic Closed timeout".into())
            } else {
                Ok(())
            }
        }
        fn logout_in_progress(&self) -> bool {
            self.submitted.load(Ordering::SeqCst)
        }
        fn logout_acknowledged(&self) -> bool {
            self.acknowledgement.load(Ordering::SeqCst)
        }
    }
    let g = DelayedLogout {
        submitted: AtomicBool::new(false),
        acknowledgement: AtomicBool::new(false),
        fail_wait: AtomicBool::new(true),
        requests: Mutex::new(0),
    };
    let mut progress = LogoutProgress::default();
    assert!(
        progress
            .execute(
                Some(&g),
                || panic!("cannot persist a timeout as completed"),
                || panic!("cannot clean up on timeout")
            )
            .is_err()
    );
    assert!(
        progress
            .execute(
                Some(&g),
                || panic!("cannot persist unverified Closed"),
                || panic!("cannot clean up without logout evidence")
            )
            .is_err()
    );
    assert!(!progress.closed && !progress.recorded);
    g.acknowledgement.store(true, Ordering::SeqCst); // Late ok / LoggingOut from same client.
    progress.execute(Some(&g), || Ok(()), || Ok(())).unwrap();
    assert!(progress.closed && progress.recorded);
    assert_eq!(*g.requests.lock().unwrap(), 1);
}
impl MockTelegramGateway {
    fn pages(pages: Vec<Value>) -> Self {
        Self {
            pages: Mutex::new(pages.into_iter().map(Ok).collect()),
            requests: Mutex::new(Vec::new()),
            cancel_history: false,
        }
    }
}
impl TelegramGateway for MockTelegramGateway {
    fn request_batch(&self, requests: Vec<Value>, cancel: &AtomicBool) -> Result<Vec<Value>> {
        requests
            .into_iter()
            .map(|body| self.request(body, cancel))
            .collect()
    }
    fn request(&self, v: Value, cancel: &AtomicBool) -> Result<Value> {
        self.requests.lock().unwrap().push(v.clone());
        match v["@type"].as_str().unwrap() {
            "getChatHistory" => {
                if self.cancel_history {
                    cancel.store(true, Ordering::Relaxed);
                    return Err("Cancelled".into());
                }
                self.pages
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(json!({"messages":[]})))
            }
            "getChat" => Ok(json!({"id":1,"type":{"@type":"chatTypePrivate","user_id":1}})),
            "getUser" => Ok(json!({"type":{"@type":"userTypeRegular"}})),
            "getMessageProperties" => Ok(
                json!({"can_be_forwarded":v["message_id"]!=13,"can_get_link":v["message_id"]!=13}),
            ),
            "getMessage" => Ok(json!({"content":{"@type":"messageText"}})),
            "getMessageLink" => Ok(json!({"link":"https://t.me/synthetic/1"})),
            "forwardMessages" => Ok(
                json!({"messages":v["message_ids"].as_array().unwrap().iter().map(|mid|if mid==&json!(2) {Value::Null} else {json!({"id":mid,"sending_state":null})}).collect::<Vec<_>>()}),
            ),
            _ => Ok(json!({"@type":"ok"})),
        }
    }
}
fn message(n: i64) -> Value {
    json!({"id":n,"chat_id":-1001,"date":n,"media_album_id":"0","content":{"@type":"messageText","text":{"text":"Synthetic"}},"interaction_info":{"view_count":n,"forward_count":0,"reply_info":{"reply_count":0},"reactions":{"reactions":[]}}})
}
fn page(messages: Vec<Value>) -> Value {
    json!({"messages":messages})
}
fn spec(count: usize) -> QuerySpec {
    QuerySpec {
        version: 1,
        chat_id: "-1001".into(),
        scope: Scope::Recent { count },
        filter: FilterExpr::And { items: vec![] },
        sort: vec![SortSpec {
            field: Field::Views,
            descending: true,
        }],
        output_limit: 1,
    }
}
fn fixture() -> Vec<TelegramMessage> {
    serde_json::from_str::<Vec<Value>>(include_str!("fixtures/messages.json"))
        .unwrap()
        .iter()
        .map(|m| normalize(m).unwrap())
        .collect()
}
struct PolicyGateway {
    base: MockTelegramGateway,
    channel: bool,
    status: Value,
    response: Option<Result<Value>>,
}
impl TelegramGateway for PolicyGateway {
    fn request_batch(&self, requests: Vec<Value>, cancel: &AtomicBool) -> Result<Vec<Value>> {
        requests
            .into_iter()
            .map(|body| self.request(body, cancel))
            .collect()
    }
    fn request(&self, v: Value, cancel: &AtomicBool) -> Result<Value> {
        match v["@type"].as_str().unwrap() {
            "getChat" => {
                self.base.requests.lock().unwrap().push(v);
                Ok(
                    json!({"id":-2001,"type":{"@type":"chatTypeSupergroup","supergroup_id":2001},"permissions":{"can_send_basic_messages":true,"can_send_photos":true}}),
                )
            }
            "getSupergroup" => Ok(json!({"is_channel":self.channel,"status":self.status})),
            "forwardMessages" if self.response.is_some() => {
                self.base.requests.lock().unwrap().push(v);
                self.response.clone().unwrap()
            }
            _ => self.base.request(v, cancel),
        }
    }
}
#[test]
fn target_channel_posting_rights_and_restricted_membership_are_enforced() {
    let key = MessageKey {
        chat_id: "-1001".into(),
        message_id: "10".into(),
    };
    let mut g = PolicyGateway {
        base: MockTelegramGateway::pages(vec![]),
        channel: true,
        status: json!({"@type":"chatMemberStatusMember"}),
        response: None,
    };
    assert!(ActionService::forward(&g, "-2001", std::slice::from_ref(&key)).is_err());
    assert!(
        g.base
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r["@type"] != "forwardMessages")
    );
    g.status =
        json!({"@type":"chatMemberStatusAdministrator","rights":{"can_post_messages":false}});
    assert!(ActionService::preview(&g, "-2001", std::slice::from_ref(&key)).is_err());
    g.status = json!({"@type":"chatMemberStatusAdministrator","rights":{"can_post_messages":true}});
    assert_eq!(
        ActionService::forward(&g, "-2001", std::slice::from_ref(&key))
            .unwrap()
            .items[0]
            .state,
        "Sent"
    );
    g.channel = false;
    g.status = json!({"@type":"chatMemberStatusRestricted","is_member":true,"permissions":{"can_send_basic_messages":false}});
    assert_eq!(
        ActionService::forward(&g, "-2001", std::slice::from_ref(&key))
            .unwrap()
            .items[0]
            .state,
        "Rejected"
    );
    g.status = json!({"@type":"chatMemberStatusLeft"});
    assert!(ActionService::forward(&g, "-2001", &[key]).is_err());
}
#[test]
fn forward_pending_failed_omitted_malformed_and_batch_errors_are_reported() {
    let keys: Vec<_> = (20..25)
        .map(|n| MessageKey {
            chat_id: "-1001".into(),
            message_id: n.to_string(),
        })
        .collect();
    let mut g = PolicyGateway {
        base: MockTelegramGateway::pages(vec![]),
        channel: true,
        status: json!({"@type":"chatMemberStatusAdministrator","rights":{"can_post_messages":true}}),
        response: Some(Ok(
            json!({"messages":[{"id":101,"sending_state":{"@type":"messageSendingStatePending"}},{"id":102,"sending_state":{"@type":"messageSendingStateFailed"}},null,{}]}),
        )),
    };
    let r = ActionService::forward(&g, "-2001", &keys).unwrap();
    assert_eq!(r.items[0].state, "Pending");
    assert_eq!(r.items[0].sent_id.as_deref(), Some("101"));
    assert!(r.items[1..].iter().all(|i| i.state == "Failed"));
    g.response = Some(Err("TDLib error 429".into()));
    let r = ActionService::forward(&g, "-2001", &keys).unwrap();
    assert_eq!(r.items.len(), 5);
    assert!(r.items.iter().all(|i| i.state == "Failed"));
}
#[test]
fn auth_steps_are_explicit_and_unsupported_states_do_not_retry() {
    let g = MockTelegramGateway::pages(vec![]);
    for (state, kind) in [
        (
            "authorizationStateWaitPhoneNumber",
            "setAuthenticationPhoneNumber",
        ),
        ("authorizationStateWaitCode", "checkAuthenticationCode"),
        (
            "authorizationStateWaitPassword",
            "checkAuthenticationPassword",
        ),
        (
            "authorizationStateWaitEmailAddress",
            "setAuthenticationEmailAddress",
        ),
        (
            "authorizationStateWaitEmailCode",
            "checkAuthenticationEmailCode",
        ),
    ] {
        AuthService::submit(&g, state, "synthetic-input".into()).unwrap();
        assert_eq!(g.requests.lock().unwrap().last().unwrap()["@type"], kind);
    }
    let before = g.requests.lock().unwrap().len();
    assert!(
        AuthService::submit(
            &g,
            "authorizationStateWaitRegistration",
            "synthetic-input".into()
        )
        .is_err()
    );
    assert_eq!(g.requests.lock().unwrap().len(), before);
}
fn cmp(field: Field, op: CompareOp, value: Scalar) -> FilterExpr {
    FilterExpr::Compare { field, op, value }
}

#[test]
fn short_page_one_then_49_reaches_50_and_output_does_not_stop_scan() {
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page((51..100).rev().map(message).collect()),
    ]);
    let mut progress = vec![];
    let r = scan(&g, &spec(50), "a".into(), &AtomicBool::new(false), |r| {
        progress.push(r)
    })
    .unwrap();
    assert_eq!(r.scanned, 50);
    assert_eq!(r.pages, 2);
    assert_eq!(r.posts.len(), 1);
    assert_eq!(r.status, ScanStatus::Complete);
    assert!(r.exact);
    assert_eq!(g.requests.lock().unwrap()[0]["limit"], 50);
    assert_eq!(g.requests.lock().unwrap()[1]["from_message_id"], 100);
    assert_eq!(progress[0].status, ScanStatus::Scanning);
    assert!(!progress[0].exact);
}
#[test]
fn duplicate_cursor_empty_and_no_progress_are_distinct() {
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page(vec![message(100), message(99)]),
        page(vec![]),
    ]);
    let r = scan(&g, &spec(50), "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.scanned, 2);
    assert_eq!(r.status, ScanStatus::Complete);
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(100)]),
    ]);
    let r = scan(&g, &spec(50), "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.status, ScanStatus::Partial);
    assert!(!r.exact);
}
#[test]
fn final_remaining_one_reserves_inclusive_cursor_slot() {
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page(vec![message(100), message(99)]),
    ]);
    let r = scan(&g, &spec(2), "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.scanned, 2);
    assert_eq!(r.status, ScanStatus::Complete);
    assert_eq!(g.requests.lock().unwrap()[1]["limit"], 2);
}
#[test]
fn cancellation_before_and_during_request_and_error_keep_results() {
    let g = MockTelegramGateway::pages(vec![]);
    let r = scan(&g, &spec(50), "a".into(), &AtomicBool::new(true), |_| {}).unwrap();
    assert_eq!(r.status, ScanStatus::Cancelled);
    assert!(g.requests.lock().unwrap().is_empty());
    let mut g = MockTelegramGateway::pages(vec![]);
    g.cancel_history = true;
    assert_eq!(
        scan(&g, &spec(50), "a".into(), &AtomicBool::new(false), |_| {})
            .unwrap()
            .status,
        ScanStatus::Cancelled
    );
    let g = MockTelegramGateway::pages(vec![page(vec![message(100)])]);
    g.pages
        .lock()
        .unwrap()
        .push_back(Err("TDLib error 429".into()));
    let r = scan(&g, &spec(50), "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.status, ScanStatus::Error);
    assert_eq!(r.scanned, 1);
    assert_eq!(r.posts.len(), 1);
}
#[test]
fn cancelled_progress_does_not_finish_complete() {
    let c = AtomicBool::new(false);
    let g = MockTelegramGateway::pages(vec![page(vec![message(100)]), page(vec![message(99)])]);
    let r = scan(&g, &spec(50), "a".into(), &c, |_| {
        c.store(true, Ordering::Relaxed)
    })
    .unwrap();
    assert_eq!(r.status, ScanStatus::Cancelled);
    assert_eq!(r.scanned, 1);
}
#[test]
fn album_cross_page_metrics_provenance_and_cross_chat() {
    let f = fixture();
    let posts = assemble(&f);
    assert_eq!(posts.len(), 4);
    let album = posts.iter().find(|p| p.members.len() == 2).unwrap();
    assert_eq!(album.metrics.views, Some(90));
    assert_eq!(album.metrics.forwards, Some(7));
    assert_eq!(album.metrics.reply_info, Some(0));
    assert_eq!(album.provenance["reactions"], "299");
    assert!(album.partial);
    let raws: Vec<Value> = serde_json::from_str(include_str!("fixtures/messages.json")).unwrap();
    let g = MockTelegramGateway::pages(vec![
        page(vec![raws[0].clone()]),
        page(vec![raws[0].clone(), raws[1].clone()]),
    ]);
    let r = scan(&g, &spec(2), "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.posts[0].members.len(), 2);
    assert_eq!(r.status, ScanStatus::Partial);
    assert!(!r.exact);
}
#[test]
fn ids_reactions_fallback_and_null_zero_semantics() {
    let f = fixture();
    assert_eq!(f[0].media_album_id.as_deref(), Some("9223372036854775806"));
    assert!(
        matches!(&f[1].metrics.reactions.as_ref().unwrap()[1].key,ReactionKey::CustomEmoji(s) if s=="9223372036854775805")
    );
    assert_eq!(
        f[1].metrics.reactions.as_ref().unwrap()[2].key,
        ReactionKey::Paid
    );
    assert_eq!(f[4].content_kind, "unsupportedfuturetype");
    let missing = &assemble(&[f[2].clone()])[0];
    let zero = &assemble(&[f[3].clone()])[0];
    let album = &assemble(&f[..2])[0];
    assert_eq!(
        evaluate(
            missing,
            &FilterExpr::IsNull {
                field: Field::ReplyInfo
            }
        ),
        Truth::True
    );
    assert_eq!(
        evaluate(
            zero,
            &FilterExpr::IsNull {
                field: Field::ReplyInfo
            }
        ),
        Truth::False
    );
    assert_eq!(
        evaluate(
            missing,
            &cmp(Field::Comments, CompareOp::Eq, Scalar::Number(0))
        ),
        Truth::Unknown
    );
    assert_eq!(
        evaluate(
            zero,
            &cmp(Field::Comments, CompareOp::Eq, Scalar::Number(0))
        ),
        Truth::True
    );
    assert_eq!(
        field_value(album, &Field::Reaction("👍".into())),
        Some(Scalar::Number(3))
    );
    assert_eq!(
        field_value(album, &Field::ReactionTotal),
        Some(Scalar::Number(6))
    );
    assert_eq!(
        field_value(zero, &Field::Reaction("👍".into())),
        Some(Scalar::Number(0))
    );
    assert_eq!(field_value(missing, &Field::Reaction("👍".into())), None);
}
#[test]
fn configurable_preset_and_kleene_not_and_or() {
    let f = fixture();
    let missing = &assemble(&[f[2].clone()])[0];
    let zero = &assemble(&[f[3].clone()])[0];
    let album = &assemble(&f[..2])[0];
    let preset = FilterExpr::Not {
        item: Box::new(FilterExpr::And {
            items: vec![
                FilterExpr::IsNull {
                    field: Field::ReplyInfo,
                },
                FilterExpr::ContainsContent {content_kind:"text".into(),exists:true},
            ],
        }),
    };
    assert_eq!(evaluate(missing, &preset), Truth::False);
    assert_eq!(evaluate(zero, &preset), Truth::True);
    assert_eq!(evaluate(album, &preset), Truth::True);
    let unknown = cmp(Field::Comments, CompareOp::Eq, Scalar::Number(0));
    assert_eq!(
        evaluate(
            missing,
            &FilterExpr::Not {
                item: Box::new(unknown.clone())
            }
        ),
        Truth::Unknown
    );
    assert_eq!(
        evaluate(
            missing,
            &FilterExpr::Or {
                items: vec![
                    unknown.clone(),
                    FilterExpr::IsNull {
                        field: Field::Comments
                    }
                ]
            }
        ),
        Truth::True
    );
    assert_eq!(
        evaluate(
            missing,
            &FilterExpr::And {
                items: vec![
                    unknown,
                    FilterExpr::IsNotNull {
                        field: Field::Comments
                    }
                ]
            }
        ),
        Truth::False
    );
}
#[test]
fn numeric_comparators_types_and_version_validation() {
    let p = &assemble(&[normalize(&message(5)).unwrap()])[0];
    for (op, n, truth) in [
        (CompareOp::Eq, 5, Truth::True),
        (CompareOp::Ne, 5, Truth::False),
        (CompareOp::Gt, 4, Truth::True),
        (CompareOp::Ge, 5, Truth::True),
        (CompareOp::Lt, 6, Truth::True),
        (CompareOp::Le, 5, Truth::True),
    ] {
        assert_eq!(
            evaluate(p, &cmp(Field::Views, op, Scalar::Number(n))),
            truth
        );
    }
    let mut s = spec(1);
    s.filter = cmp(Field::Views, CompareOp::Eq, Scalar::Text("bad".into()));
    assert!(s.validate().is_err());
    s = spec(1);
    s.version = 2;
    assert!(s.validate().is_err());
    s = spec(0);
    assert!(s.validate().is_err());
}
#[test]
fn stable_sort_both_directions_null_last_and_multiple_keys() {
    let mut ms = vec![
        normalize(&message(10)).unwrap(),
        normalize(&message(11)).unwrap(),
        fixture()[2].clone(),
    ];
    let mut s = spec(3);
    s.output_limit = 10;
    assert_eq!(
        query(&ms, &s)
            .iter()
            .map(|p| p.representative.message_id.as_str())
            .collect::<Vec<_>>(),
        vec!["11", "10", "298"]
    );
    s.sort[0].descending = false;
    assert_eq!(query(&ms, &s)[0].representative.message_id, "10");
    assert_eq!(query(&ms, &s)[2].representative.message_id, "298");
    ms[1].metrics.views = Some(10);
    ms[0].date = 1;
    ms[1].date = 1;
    ms.reverse();
    assert_eq!(query(&ms, &s)[0].representative.message_id, "10");
    s.sort.push(SortSpec {
        field: Field::Date,
        descending: true,
    });
    ms[0].metrics.views = Some(10);
    assert_eq!(query(&ms, &s)[0].representative.message_id, "298");
}
#[test]
fn date_scope_half_open_budget_partial_and_lower_boundary_complete() {
    let mut s = spec(10);
    s.scope = Scope::Dates {
        from: 95,
        to: 99,
        budget: 10,
    };
    s.output_limit = 20;
    let g = MockTelegramGateway::pages(vec![page((90..=100).rev().map(message).collect())]);
    let r = scan(&g, &s, "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.status, ScanStatus::Complete);
    assert_eq!(r.posts.len(), 4);
    assert!(r.posts.iter().all(|p| p.date >= 95 && p.date < 99));
    s.scope = Scope::Dates {
        from: 1,
        to: 99,
        budget: 2,
    };
    let g = MockTelegramGateway::pages(vec![page(vec![message(100), message(99)])]);
    let r = scan(&g, &s, "a".into(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.status, ScanStatus::Partial);
    assert!(!r.exact);
}
#[test]
fn forward_grouped_sorted_100_protected_and_null_partial_failure() {
    let g = MockTelegramGateway::pages(vec![]);
    let mut keys: Vec<_> = (1..=202)
        .rev()
        .map(|n| MessageKey {
            chat_id: "-1001".into(),
            message_id: n.to_string(),
        })
        .collect();
    keys.push(MessageKey {
        chat_id: "-1002".into(),
        message_id: "10".into(),
    });
    keys.push(keys[0].clone());
    let r = ActionService::forward(&g, "1", &keys).unwrap();
    assert_eq!(r.items.len(), 203);
    assert_eq!(r.items.iter().filter(|i| i.state == "Rejected").count(), 1);
    assert_eq!(r.items.iter().filter(|i| i.state == "Failed").count(), 1);
    let requests = g.requests.lock().unwrap();
    let batches: Vec<_> = requests
        .iter()
        .filter(|r| r["@type"] == "forwardMessages")
        .collect();
    assert_eq!(batches.len(), 4);
    for b in batches {
        let ids = b["message_ids"].as_array().unwrap();
        assert!(ids.len() <= 100);
        assert!(
            ids.windows(2)
                .all(|w| w[0].as_i64().unwrap() < w[1].as_i64().unwrap())
        );
        assert!(!ids.contains(&json!(13)));
        assert_eq!(b["send_copy"], false);
    }
}
#[test]
fn link_capability_checked_album_flag_and_denial() {
    let g = MockTelegramGateway::pages(vec![]);
    let key = MessageKey {
        chat_id: "-1001".into(),
        message_id: "10".into(),
    };
    assert!(
        ActionService::link(&g, &key, true)
            .unwrap()
            .starts_with("https://t.me/")
    );
    assert_eq!(g.requests.lock().unwrap()[1]["for_album"], true);
    assert!(
        ActionService::link(
            &g,
            &MessageKey {
                message_id: "13".into(),
                ..key
            },
            false
        )
        .is_err()
    );
}
#[test]
fn forwarding_preview_never_sends_and_lists_rejections() {
    let g = MockTelegramGateway::pages(vec![]);
    let keys = vec![
        MessageKey {
            chat_id: "-1001".into(),
            message_id: "13".into(),
        },
        MessageKey {
            chat_id: "-1001".into(),
            message_id: "14".into(),
        },
    ];
    let r = ActionService::preview(&g, "1", &keys).unwrap();
    assert_eq!(r.items[0].state, "Rejected");
    assert_eq!(r.items[1].state, "Allowed");
    assert!(
        g.requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r["@type"] != "forwardMessages")
    );
}
struct BootstrapGateway {
    requests: Mutex<Vec<Value>>,
    proxy: bool,
    failure: Option<&'static str>,
}
impl TelegramGateway for BootstrapGateway {
    fn request(&self, _: Value, _: &AtomicBool) -> Result<Value> {
        Err("Proxy cannot complete before parameters; bootstrap must submit a batch".into())
    }
    fn request_batch(&self, requests: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> {
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]["@type"],
            if self.proxy {
                "addProxy"
            } else {
                "disableProxy"
            }
        );
        assert_eq!(requests[1]["@type"], "setTdlibParameters");
        *self.requests.lock().unwrap() = requests;
        if let Some(error) = self.failure {
            return Err(error.into());
        }
        Ok(vec![
            if self.proxy {
                json!({"@type":"addedProxy"})
            } else {
                json!({"@type":"ok"})
            },
            json!({"@type":"ok"}),
        ])
    }
}
#[test]
fn proxy_and_no_proxy_bootstrap_submit_parameters_without_waiting_for_proxy() {
    for (proxy, failure) in [
        (true, None),
        (false, None),
        (true, Some("setTdlibParameters: TDLib error 400")),
        (false, Some("disableProxy: TDLib error 500")),
    ] {
        let g = BootstrapGateway {
            requests: Mutex::new(Vec::new()),
            proxy,
            failure,
        };
        let root = std::env::temp_dir().join(format!(
            "tmsgpicker-bootstrap-test-{}-{proxy}-{}",
            std::process::id(),
            failure.is_some()
        ));
        let config = ConnectConfig {
            api_id: Some(1),
            api_hash: Some("synthetic-not-a-credential".into()),
            passphrase: "synthetic-passphrase".into(),
            proxy: proxy.then_some(ProxyConfig {
                server: "localhost".into(),
                port: 9999,
                username: "".into(),
                password: "".into(),
            }),
        };
        let result = AuthService::initialize(&g, config, &root);
        if let Some(error) = failure {
            assert!(result.unwrap_err().contains(error));
        } else {
            result.unwrap();
        }
        let r = g.requests.lock().unwrap();
        if proxy {
            assert_eq!(r[0]["@type"], "addProxy");
            assert_eq!(r[0]["proxy"]["port"], 9999);
        }
        assert_eq!(r[1]["@type"], "setTdlibParameters");
        assert_ne!(r[1]["database_encryption_key"], "synthetic-passphrase");
        // Remove only the uniquely named synthetic test folders we just created.
        std::fs::remove_dir(root.join("db")).unwrap();
        std::fs::remove_dir(root.join("files")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

#[test]
fn transient_cursor_only_pages_are_retried_before_marking_partial() {
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(100), message(99)]),
    ]);
    let r = scan(
        &g,
        &spec(2),
        "retry".into(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.scanned, 2);
    assert_eq!(r.status, ScanStatus::Complete);
    assert_eq!(r.pages, 3);
}

#[test]
fn permanently_stagnant_cursor_is_bounded() {
    let g = MockTelegramGateway::pages(vec![
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(100)]),
        page(vec![message(99)]),
    ]);
    let r = scan(
        &g,
        &spec(2),
        "retry".into(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.status, ScanStatus::Partial);
    assert_eq!(r.scanned, 1);
    assert_eq!(r.pages, 4);
}

#[test]
fn chat_list_includes_archive_and_supports_scoped_search() {
    struct ChatsMock {
        requests: Mutex<Vec<Value>>,
    }
    impl TelegramGateway for ChatsMock {
        fn request(&self, request: Value, _: &AtomicBool) -> Result<Value> {
            self.requests.lock().unwrap().push(request.clone());
            let kind = request["@type"].as_str().unwrap_or_default();
            let archive = request["chat_list"]["@type"] == "chatListArchive";
            match kind {
                "loadChats" => Ok(json!({"@type":"ok"})),
                "getChats" => {
                    if archive { Ok(json!({"chat_ids":[2,3]})) }
                    else { Ok(json!({"chat_ids":[1,2]})) }
                }
                "searchChats" => Ok(json!({"chat_ids":[1]})),
                "getChat" => {
                    let id=request["chat_id"].as_i64().unwrap();
                    let title = match id { 1 => "Main One", 2 => "Archive Two", _ => "Archive Three" };
                    let list = if id == 1 { "chatListMain" } else { "chatListArchive" };
                    Ok(json!({"id":id,"title":title,"type":{"@type":"chatTypeSupergroup"},
                        "positions":[{"list":{"@type":list},"order":"100"}]}))
                }
                _ => panic!("unexpected TDLib request: {kind}"),
            }
        }
        fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> { unreachable!() }
    }
    let g = ChatsMock { requests: Mutex::new(vec![]) };
    let all = ChatService::list(&g, "", ChatListScope::All).unwrap();
    assert_eq!(all.len(),3);
    assert_eq!(all.iter().filter(|c| c.archived).count(),2);
    assert_eq!(all.iter().filter(|c| c.id=="2").count(),1);
    let archived = ChatService::list(&g,"Three",ChatListScope::Archive).unwrap();
    assert_eq!(archived.len(),1);
    assert_eq!(archived[0].title,"Archive Three");
    let main = ChatService::list(&g,"",ChatListScope::Main).unwrap();
    assert_eq!(main.len(),1);
    assert_eq!(main[0].title,"Main One");
    assert!(g.requests.lock().unwrap().iter().any(|v| v["@type"]=="loadChats"
        && v["chat_list"]["@type"]=="chatListArchive"));
}

#[test]
fn recent_n_counts_complete_media_group_as_one_logical_post() {
    let album = |id:i64| {
        let mut msg=message(id);
        msg["media_album_id"]=json!(987654321_i64);
        msg["content"]=json!({"@type":"messagePhoto","caption":{"text":"album"},"photo":{"sizes":[]}});
        msg
    };
    let g=MockTelegramGateway::pages(vec![
        page(vec![message(112),album(111)]),
        page(vec![album(111),album(110),album(109)]),
        page(vec![album(109),message(108)]),
    ]);
    let mut spec=spec(2);spec.output_limit=8;
    let r=scan(&g,&spec,"album-logical".into(),&AtomicBool::new(false),|_|{}).unwrap();
    assert_eq!(r.scanned,4); // one text + three album members
    assert_eq!(r.posts.len(),2);
    assert_eq!(r.posts.iter().find(|p|p.kind==PostKind::Album).unwrap().members.len(),3);
    assert!(r.posts.iter().all(|p|p.members.iter().all(|m|m.key.message_id!="108")));
    assert_eq!(r.pages,3);
    assert_eq!(g.requests.lock().unwrap()[0]["limit"],2);
}
#[test]
fn recent_single_message_still_stops_at_exact_count() {
    let g=MockTelegramGateway::pages(vec![page((90..=100).rev().map(message).collect())]);
    let mut spec=spec(3);spec.output_limit=5;
    let r=scan(&g,&spec,"singles".into(),&AtomicBool::new(false),|_|{}).unwrap();
    assert_eq!(r.scanned,3);
    assert_eq!(r.posts.len(),3);
    assert_eq!(r.pages,1);
}
#[test]
fn normalize_media_exposes_video_preview_and_full_photo_separately() {
    let mut video=message(5);
    video["content"]=json!({"@type":"messageVideo","video":{"video":{"id":71},"thumbnail":{"file":{"id":72},"format":{"@type":"thumbnailFormatMpeg4"}},"minithumbnail":{"data":"/9j/2Q=="}}});
    let m=normalize(&video).unwrap();
    assert_eq!(m.thumbnail_file_id,Some(72));
    assert_eq!(m.full_file_id,Some(71));
    let mut photo=message(6);
    photo["content"]=json!({"@type":"messagePhoto","photo":{"sizes":[{"width":64,"height":64,"photo":{"id":17}},{"width":1280,"height":720,"photo":{"id":20}},{"width":320,"height":320,"photo":{"id":18}}]}});
    let p=normalize(&photo).unwrap();
    assert_eq!(p.thumbnail_file_id,Some(18));
    assert_eq!(p.full_file_id,Some(20));
}

#[test]
fn account_media_accepts_tdlib_owned_file_and_database_cache_but_denies_external_paths() {
    struct MediaMock { path: String }
    impl TelegramGateway for MediaMock {
        fn request(&self, request: Value, _: &AtomicBool) -> Result<Value> {
            assert_eq!(request["@type"], "downloadFile");
            Ok(json!({"local":{"is_downloading_completed":true,"path":self.path}}))
        }
        fn request_batch(&self, _: Vec<Value>, _: &AtomicBool) -> Result<Vec<Value>> { unreachable!() }
    }
    let tmp = std::env::temp_dir().join(format!(
        "tmsg-media-path-{}-{}",std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let session=tmp.join("session");
    std::fs::create_dir_all(session.join("db")).unwrap();
    std::fs::create_dir_all(session.join("files")).unwrap();
    let image=[0xff,0xd8,0xff,0xd9];
    let db_file=session.join("db").join("image.jpg");
    let file=session.join("files").join("image.jpg");
    let outside=tmp.join("outside.jpg");
    for path in [&db_file,&file,&outside] {std::fs::write(path,image).unwrap();}
    for path in [&db_file,&file] {
        let mock=MediaMock{path:path.to_string_lossy().into_owned()};
        assert!(ChatService::media_data(&mock,1,&session.join("files"),false).unwrap().unwrap().starts_with("data:image/jpeg;"));
    }
    let relative=MediaMock{path:"image.jpg".into()};
    assert!(ChatService::media_data(&relative,1,&session.join("files"),false).unwrap().is_some());
    let forbidden=MediaMock{path:outside.to_string_lossy().into_owned()};
    assert!(ChatService::media_data(&forbidden,1,&session.join("files"),false).is_err());
    std::fs::remove_dir_all(tmp).unwrap();
}

#[test]
fn post_metrics_choose_max_views_forwards_and_caption_owner_interactions() {
    let mut a=normalize(&message(100)).unwrap();
    let mut b=normalize(&message(101)).unwrap();
    let mut c=normalize(&message(102)).unwrap();
    for m in [&mut a,&mut b,&mut c] {m.media_album_id=Some("400".into());m.text.clear();m.content_kind="photo".into();}
    b.text="Caption".into();
    a.metrics.views=Some(100);b.metrics.views=Some(85);c.metrics.views=Some(150);
    a.metrics.forwards=Some(20);b.metrics.forwards=Some(9);c.metrics.forwards=Some(15);
    a.metrics.reply_info=Some(100);b.metrics.reply_info=Some(7);c.metrics.reply_info=Some(44);
    a.metrics.reactions=Some(vec![]);b.metrics.reactions=None;c.metrics.reactions=Some(vec![]);
    let post=&assemble(&[a,b,c])[0];
    assert_eq!(post.metrics.views,Some(150));
    assert_eq!(post.metrics.forwards,Some(20));
    assert_eq!(post.metrics.reply_info,Some(7));
    assert!(post.metrics.reactions.is_none());
    assert_eq!(post.text,"Caption");
    assert_eq!(post.provenance["views"],"102");
    assert_eq!(post.provenance["forwards"],"100");
    assert_eq!(post.provenance["reactions"],"101");
}
#[test]
fn combined_content_predicates_apply_to_entire_post() {
    let mut a=normalize(&message(100)).unwrap();
    let mut b=normalize(&message(101)).unwrap();
    for m in [&mut a,&mut b]{m.media_album_id=Some("40".into());}
    a.content_kind="video".into();b.content_kind="document".into();
    let post=&assemble(&[a,b])[0];
    let filter=FilterExpr::And{items:vec![
        FilterExpr::ContainsContent{content_kind:"video".into(),exists:true},
        FilterExpr::ContainsContent{content_kind:"photo".into(),exists:false},
    ]};
    assert_eq!(evaluate(post,&filter),Truth::True);
    assert_eq!(evaluate(post,&FilterExpr::ContainsContent{content_kind:"document".into(),exists:false}),Truth::False);
    let spec=QuerySpec{filter, ..spec(10)};
    assert!(spec.validate().is_ok());
    let invalid=QuerySpec{filter:FilterExpr::ContainsContent{content_kind:"garbage".into(),exists:true}, ..spec(10)};
    assert!(invalid.validate().is_err());
}
