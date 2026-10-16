//! Bounded TDLib history paging with a monotone cursor and global message dedupe.
use crate::core::{normalize,TelegramMessage};
use crate::gateway::{id, Result, TelegramGateway};
use serde_json::json;
use std::{collections::HashSet,sync::atomic::AtomicBool,time::Duration};

pub enum Page {
    Items(Vec<TelegramMessage>),
    Exhausted,
    Stalled,
}

pub struct HistoryReader<'a> {
    gateway:&'a dyn TelegramGateway,
    chat:i64,
    cursor:i64,
    offset:i32,
    anchor_pending:bool,
    from_head:bool,
    // A history reader scans one chat, so message IDs alone are sufficient.
    // This avoids storing two heap-allocated strings per scanned raw message.
    seen:HashSet<i64>,
    stagnant:u8,
    pub pages:usize,
    pub raw_messages_read:usize,
}

impl<'a> HistoryReader<'a> {
    pub fn new(gateway:&'a dyn TelegramGateway,chat_id:&str,scope:&crate::core::Scope,cancel:&AtomicBool)->Result<Self> {
        let chat=id(chat_id)?;
        let mut cursor=0;
        let mut offset=0;
        let mut anchor_pending=false;
        if let crate::core::Scope::Dates{to,..}=scope {
            // Safe optimization: look back from an upper-date anchor with up to
            // 99 newer neighbors. Reject the anchor if no out-of-range head
            // can be observed: that would silently lose eligible newer Posts.
            if let Ok(anchor)=gateway.request(json!({"@type":"getChatMessageByDate","chat_id":chat,
                "date":to.saturating_sub(1)}),cancel)
                && let Some(mid)=anchor["id"].as_i64().filter(|v|*v>0)
                && anchor["chat_id"].as_i64()==Some(chat)
            {
                cursor=mid;
                offset = -99;
                anchor_pending=true;
            }
        }
        Ok(Self{gateway,chat,cursor,offset,anchor_pending,
            from_head:!anchor_pending,seen:HashSet::new(),stagnant:0,pages:0,raw_messages_read:0})
    }
    pub fn from_head(&self)->bool {self.from_head}
    pub fn next_page(&mut self,hint:usize,to_bound:Option<i64>,cancel:&AtomicBool)->Result<Page>{
        loop {
            let limit=if self.anchor_pending {100} else {
                (hint.saturating_add(usize::from(self.cursor!=0))).clamp(1,100)
            };
            let was_anchor=self.anchor_pending;
            let raw=self.gateway.request(json!({"@type":"getChatHistory","chat_id":self.chat,
                "from_message_id":self.cursor,"offset":self.offset,"limit":limit,
                "only_local":false}),cancel)?;
            self.pages+=1;
            let list=raw["messages"].as_array().ok_or("Invalid history response")?;
            if self.anchor_pending {
                self.anchor_pending=false;
                let first=list.iter().find(|v|!v.is_null());
                // A date seek is trusted only when the returned window visibly
                // crosses the requested upper boundary. Otherwise restart at
                // HEAD, even if it costs more raw messages.
                let includes_anchor=list.iter().any(|v|v["id"].as_i64()==Some(self.cursor));
                if !includes_anchor || !first.is_some_and(|v| v["chat_id"].as_i64()==Some(self.chat)
                    && v["date"].as_i64().zip(to_bound).is_some_and(|(d,to)|d>=to)) {
                    self.cursor=0;self.offset=0;self.from_head=true;
                    self.stagnant=0;
                    continue;
                }
                self.offset=0;
            }
            if list.is_empty(){return Ok(Page::Exhausted);}
            let mut next=self.cursor;
            let mut out=Vec::new();
            for v in list.iter().filter(|v|!v.is_null()) {
                let mid=v["id"].as_i64()
                    .or_else(||v["id"].as_str()?.parse().ok())
                    .filter(|v|*v>0)
                    .ok_or("History returned an invalid message ID")?;
                let chat=v["chat_id"].as_i64()
                    .or_else(||v["chat_id"].as_str()?.parse().ok())
                    .ok_or("History returned an invalid chat ID")?;
                if chat!=self.chat {return Err("History returned another chat".into());}
                if self.cursor!=0 && mid>=self.cursor && !was_anchor {continue;}
                // For an anchored first window, accept newer messages once;
                // later requests must move strictly backward.
                if next==0 || mid<next {next=mid;}
                // Check before costly normalization; cursor-overlap pages often
                // repeat messages already parsed on the previous request.
                if self.seen.contains(&mid){continue;}
                let m=normalize(v)?;
                self.seen.insert(mid);
                out.push(m);
                self.raw_messages_read+=1;
            }
            if next==0 || (!was_anchor && self.cursor!=0 && next>=self.cursor) {
                if self.stagnant<2 {
                    self.stagnant+=1;
                    std::thread::sleep(Duration::from_millis(120));
                    continue;
                }
                return Ok(Page::Stalled);
            }
            self.stagnant=0;self.cursor=next;
            if !out.is_empty(){return Ok(Page::Items(out));}
        }
    }
}
