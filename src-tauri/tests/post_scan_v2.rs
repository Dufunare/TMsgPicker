use serde_json::{Value,json};
use std::{collections::VecDeque,sync::{Mutex,atomic::AtomicBool},time::Duration};
use tmsgpicker_lib::{core::*,gateway::{TelegramGateway,Result},scan::{ScanLimits,scan_with_limits}};

struct MockHistory {
    pages:Mutex<VecDeque<Value>>,
    calls:Mutex<Vec<Value>>,
    date_anchor:Option<Value>,
}
impl MockHistory {
    fn new(pages:Vec<Vec<Value>>)->Self {
        Self{pages:Mutex::new(pages.into_iter().map(|v|json!({"messages":v})).collect()),
            calls:Mutex::new(Vec::new()),date_anchor:None}
    }
}
impl TelegramGateway for MockHistory {
    fn request(&self,call:Value,_:&AtomicBool)->Result<Value>{
        self.calls.lock().unwrap().push(call.clone());
        match call["@type"].as_str() {
            Some("getChatHistory")=>Ok(self.pages.lock().unwrap().pop_front().unwrap_or_else(||json!({"messages":[]}))),
            Some("getChatMessageByDate")=>Ok(self.date_anchor.clone().unwrap_or_else(||json!({"@type":"ok"}))),
            _=>Err("Unexpected mock request".into()),
        }
    }
    fn request_batch(&self,_:Vec<Value>,_:&AtomicBool)->Result<Vec<Value>>{unreachable!()}
}
fn msg(n:i64)->Value {
    json!({"id":n,"chat_id":-1001,"date":n,"media_album_id":0,
        "content":{"@type":"messageText","text":{"text":"Synthetic"}},
        "interaction_info":{"view_count":n,"forward_count":0,"reply_info":null,"reactions":null}})
}
fn album(n:i64,album_id:i64)->Value {
    let mut v=msg(n);
    v["media_album_id"]=json!(album_id);
    v["content"]=json!({"@type":"messagePhoto","photo":{"sizes":[]},"caption":{"text":"Synthetic album"}});
    v
}
fn spec(scope:Scope)->QuerySpec {
    QuerySpec{version:2,chat_id:"-1001".into(),scope,
        filter:FilterExpr::And{items:vec![]},sort:vec![SortSpec{field:Field::Date,descending:true}],
        output_limit:100}
}
fn execute(g:&MockHistory,spec:QuerySpec)->ScanResult {
    scan(g,&spec,"post-v2".into(),&AtomicBool::new(false),|_|{}).unwrap()
}

#[test]
fn recent_two_posts_across_three_history_pages_return_whole_album() {
    let g=MockHistory::new(vec![
        vec![msg(112),album(111,5)],
        vec![album(111,5),album(110,5),album(109,5)],
        vec![album(109,5),msg(108)],
    ]);
    let r=execute(&g,spec(Scope::Recent{count:2}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.stop_reason,Some(StopReason::PostLimitReached));
    assert_eq!(r.coverage.examined_posts,2);
    assert_eq!(r.stats.raw_messages_read,5);
    assert_eq!(r.posts.len(),2);
    let a=r.posts.iter().find(|p|p.kind==PostKind::Album).unwrap();
    assert_eq!(a.members.len(),3);
    assert_eq!(a.completeness.level,CompletenessLevel::Assumed);
    assert_eq!(g.calls.lock().unwrap()[0]["limit"],2);
}

#[test]
fn date_range_counts_posts_not_messages_and_keeps_grouped_caption() {
    let g=MockHistory::new(vec![
        vec![msg(103),album(99,7),album(98,7)],
        vec![album(98,7),album(97,7),msg(96),msg(95)],
    ]);
    let r=execute(&g,spec(Scope::Dates{from:95,to:100,limit_posts:Some(2)}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.coverage.requested_posts,Some(2));
    assert_eq!(r.coverage.examined_posts,2);
    assert_eq!(r.coverage.date_window_exhausted,Some(false));
    let album=r.posts.iter().find(|p|p.kind==PostKind::Album).unwrap();
    assert_eq!(album.members.len(),3);
    assert_eq!(album.date,99);
    assert_eq!(r.stats.posts_matched,2);
    assert_eq!(r.stats.posts_returned,2);
}

#[test]
fn album_with_any_member_after_upper_bound_is_excluded_whole() {
    let g=MockHistory::new(vec![
        vec![album(105,7),album(98,7),msg(97),msg(94)],
    ]);
    let r=execute(&g,spec(Scope::Dates{from:95,to:100,limit_posts:None}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.coverage.date_window_exhausted,Some(true));
    assert_eq!(r.coverage.examined_posts,1);
    assert_eq!(r.posts.len(),1);
    assert_eq!(r.posts[0].representative.message_id,"97");
}

#[test]
fn album_crossing_lower_date_bound_keeps_older_member_and_stops_after_post() {
    let g=MockHistory::new(vec![vec![album(108,31),album(98,31),msg(97)]]);
    let r=execute(&g,spec(Scope::Dates{from:100,to:110,limit_posts:None}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.stop_reason,Some(StopReason::DateLowerBoundReached));
    assert_eq!(r.coverage.examined_posts,1);
    assert_eq!(r.coverage.date_window_exhausted,Some(true));
    assert_eq!(r.posts.len(),1);
    assert_eq!(r.posts[0].members.len(),2);
    assert_eq!(r.posts[0].date,108);
}

#[test]
fn date_scope_without_post_limit_exhausts_history() {
    let g=MockHistory::new(vec![vec![msg(99),msg(98)],vec![]]);
    let r=execute(&g,spec(Scope::Dates{from:50,to:110,limit_posts:None}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.stop_reason,Some(StopReason::HistoryExhausted));
    assert_eq!(r.coverage.examined_posts,2);
    assert_eq!(r.coverage.date_window_exhausted,Some(true));
}

#[test]
fn raw_resource_limit_never_masquerades_as_post_scope_complete() {
    let g=MockHistory::new(vec![vec![album(103,12),album(102,12),album(101,12),msg(100)]]);
    let limits=ScanLimits{max_raw_messages:2,max_boundary_messages:1,
        max_pages:10,max_duration:Duration::from_secs(10)};
    let r=scan_with_limits(&g,&spec(Scope::Recent{count:2}),"bounded".into(),
        &AtomicBool::new(false),limits,|_|{}).unwrap();
    assert_eq!(r.status,ScanStatus::BudgetExceeded);
    assert_eq!(r.stop_reason,Some(StopReason::ResourceLimitReached));
    assert!(!r.coverage.scope_complete);
    assert_eq!(r.posts.len(),1);
    assert_eq!(r.posts[0].completeness.level,CompletenessLevel::Unknown);
}

#[test]
fn anchored_date_scan_requires_a_newer_outside_boundary() {
    let mut g=MockHistory::new(vec![
        vec![msg(101),msg(100),msg(99)],
        vec![msg(99),msg(98),msg(97)],
    ]);
    g.date_anchor=Some(msg(99));
    let r=execute(&g,spec(Scope::Dates{from:95,to:100,limit_posts:Some(2)}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.coverage.examined_posts,2);
    assert!(r.posts.iter().all(|p|p.date<100));
    let calls=g.calls.lock().unwrap();
    assert_eq!(calls[0]["@type"],"getChatMessageByDate");
    assert_eq!(calls[1]["offset"],-99);
    assert_eq!(calls[1]["limit"],100);
}

#[test]
fn date_anchor_without_upper_bound_evidence_restarts_from_history_head() {
    let mut g=MockHistory::new(vec![
        vec![msg(99)], // cannot know whether there are newer in-scope Posts
        vec![msg(100),msg(99),msg(98)],
    ]);
    g.date_anchor=Some(msg(99));
    let r=execute(&g,spec(Scope::Dates{from:95,to:100,limit_posts:Some(2)}));
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.coverage.examined_posts,2);
    let calls=g.calls.lock().unwrap();
    assert_eq!(calls[1]["offset"],-99);
    assert_eq!(calls[2]["from_message_id"],0);
}

#[test]
fn output_limit_does_not_shrink_scanned_scope() {
    let g=MockHistory::new(vec![vec![msg(100),msg(99),msg(98),msg(97)]]);
    let mut spec=spec(Scope::Recent{count:3});
    spec.output_limit=1;
    spec.filter=FilterExpr::Compare{field:Field::Views,op:CompareOp::Gt,
        value:Scalar::Number(98)};
    let r=execute(&g,spec);
    assert_eq!(r.coverage.examined_posts,3);
    assert_eq!(r.stats.posts_matched,2);
    assert_eq!(r.stats.posts_returned,1);
}

#[test]
fn v1_date_budget_cannot_be_reinterpreted_as_v2_post_limit() {
    let old=json!({"version":1,"chat_id":"-1001",
        "scope":{"kind":"dates","from":95,"to":100,"budget":10},
        "filter":{"kind":"and","items":[]},"sort":[],"output_limit":10});
    let parsed:Result<QuerySpec>=serde_json::from_value(old.clone()).map_err(|e|e.to_string());
    assert!(parsed.is_err());
    let mut corrected=old;
    corrected["version"]=json!(2);
    corrected["scope"]=json!({"kind":"dates","from":95,"to":100,"limit_posts":10});
    let query:QuerySpec=serde_json::from_value(corrected).unwrap();
    assert!(query.validate().is_ok());
}
