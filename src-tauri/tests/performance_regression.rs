//! Deterministic high-volume Mock tests. No TDLib binary, login or network.
use serde_json::{Value,json};
use std::sync::{Mutex,atomic::AtomicBool};
use tmsgpicker_lib::{
    core::{CompareOp,Field,FilterExpr,QuerySpec,ScanStatus,Scope,SortSpec,Scalar},
    gateway::{Result,TelegramGateway},
    scan,
    services::{ChatListScope,ChatService},
};

#[derive(Default)]
struct LargeHistory {
    calls:Mutex<usize>,
}
impl TelegramGateway for LargeHistory {
    fn request(&self,v:Value,_:&AtomicBool)->Result<Value> {
        assert_eq!(v["@type"],"getChatHistory");
        *self.calls.lock().unwrap()+=1;
        let from=v["from_message_id"].as_i64().unwrap();
        let limit=v["limit"].as_u64().unwrap() as i64;
        let newest=if from==0 {12_000} else {from-1};
        let end=(newest-limit+1).max(1);
        let messages=if newest<1 {vec![]} else {(end..=newest).rev().map(|id|json!({
            "id":id,"chat_id":-1001,"date":id,
            "media_album_id":0,
            "content":{"@type":"messageText","text":{"text":"A moderately sized synthetic Post body"}},
            "interaction_info":{"view_count":(id*37)%1000,"forward_count":0,
                "reply_info":null,"reactions":null}
        })).collect::<Vec<_>>()};
        Ok(json!({"messages":messages}))
    }
    fn request_batch(&self,_:Vec<Value>,_:&AtomicBool)->Result<Vec<Value>>{unreachable!()}
}
#[test]
fn twelve_thousand_posts_keep_progress_payload_bounded_and_final_order_correct() {
    let g=LargeHistory::default();
    let spec=QuerySpec {
        version:2,chat_id:"-1001".into(),scope:Scope::Recent{count:12_000},
        filter:FilterExpr::Compare{
            field:Field::Views,op:CompareOp::Ge,value:Scalar::Number(0),
        },
        sort:vec![SortSpec{field:Field::Views,descending:true}],
        output_limit:1000,
    };
    let cancel=AtomicBool::new(false);
    let mut updates=Vec::new();
    let r=scan::scan(&g,&spec,"stress".into(),&cancel,|partial|{
        assert_eq!(partial.status,ScanStatus::Running);
        assert!(partial.posts.len()<=40,"IPC progress should be capped at 40 Posts");
        updates.push((partial.coverage.examined_posts,partial.stats.posts_matched));
    }).unwrap();
    assert_eq!(r.status,ScanStatus::Complete);
    assert_eq!(r.coverage.examined_posts,12_000);
    assert_eq!(r.stats.raw_messages_read,12_000);
    assert_eq!(r.stats.posts_matched,12_000);
    assert_eq!(r.posts.len(),1000);
    assert!(!updates.is_empty());
    // All score ties sort by oldest timestamp first, exactly as the full-sort API.
    let mut expected:Vec<(i64,i64)>=(1..=12_000).map(|i|(i,(i*37)%1000)).collect();
    expected.sort_by_key(|(id,views)|(-views,*id));
    let keys:Vec<_>=r.posts.iter().map(|p|p.representative.message_id.clone()).collect();
    let want:Vec<_>=expected.iter().take(1000).map(|(id,_)|id.to_string()).collect();
    assert_eq!(keys,want);
    assert_eq!(*g.calls.lock().unwrap(),120);
}

#[derive(Default)]
struct BatchedChats {
    batch_sizes:Mutex<Vec<usize>>,
}
impl TelegramGateway for BatchedChats {
    fn request(&self,v:Value,_:&AtomicBool)->Result<Value>{
        match v["@type"].as_str(){
            Some("loadChats")=>Err("TDLib error 404".into()),
            Some("getChats")=>Ok(json!({"chat_ids":(1..=70).collect::<Vec<_>>()})),
            _=>Err("Unexpected non-batched getChat".into()),
        }
    }
    fn request_batch(&self,requests:Vec<Value>,_:&AtomicBool)->Result<Vec<Value>>{
        self.batch_sizes.lock().unwrap().push(requests.len());
        Ok(requests.iter().map(|req|{
            assert_eq!(req["@type"],"getChat");
            let id=req["chat_id"].as_i64().unwrap();
            json!({"id":id,"title":format!("Chat {id}"),
                "type":{"@type":"chatTypePrivate"},"positions":[]})
        }).collect())
    }
}
#[test]
fn bulk_chat_refresh_uses_bounded_batches_and_preserves_order() {
    let g=BatchedChats::default();
    let chats=ChatService::list(&g,"",ChatListScope::Main).unwrap();
    assert_eq!(chats.len(),70);
    assert_eq!(chats[0].id,"1");
    assert_eq!(chats[69].id,"70");
    assert_eq!(*g.batch_sizes.lock().unwrap(),vec![32,32,6]);
}
