//! Streaming Post assembler. TDLib messages arrive newest-first; an Album
//! can span any number of pages without being silently split.
use crate::core::{assemble, CompletenessLevel, LogicalPost, PostCompleteness, TelegramMessage,PostKind};
fn group_key(m:&TelegramMessage)->String {
    m.media_album_id.as_ref()
        .map(|album|format!("{}:a{album}",m.key.chat_id))
        .unwrap_or_else(||format!("{}:m{}",m.key.chat_id,m.key.message_id))
}
#[derive(Default)]
pub struct PostAccumulator {
    current:Vec<TelegramMessage>,
    current_key:Option<String>,
    // The history HEAD is a known newest boundary; date seek is not.
    upper_edge_seen:bool,
}
impl PostAccumulator {
    pub fn new(from_head:bool)->Self {Self{upper_edge_seen:from_head,..Self::default()}}
    pub fn is_open(&self)->bool {!self.current.is_empty()}
    pub fn mark_head(&mut self){if self.current.is_empty(){self.upper_edge_seen=true;}}
    /// Finalize standalone messages immediately; finalize Albums only when
    /// their *next* Post is observed, or true history exhaustion is reached.
    pub fn push(&mut self,m:TelegramMessage)->Vec<LogicalPost> {
        let key=group_key(&m);
        let mut ready=Vec::new();
        if m.media_album_id.is_none() {
            if let Some(prev)=self.take_post(true) {ready.push(prev);}
            self.upper_edge_seen=true;
            self.current_key=Some(key);
            self.current.push(m);
            if let Some(single)=self.take_post(true){ready.push(single);}
            return ready;
        }
        if self.current_key.as_deref()==Some(key.as_str()) {
            if !self.current.iter().any(|old|old.key==m.key) {self.current.push(m);}
            return ready;
        }
        if let Some(prev)=self.take_post(true){ready.push(prev);self.upper_edge_seen=true;}
        self.current_key=Some(key);
        self.current.push(m);
        ready
    }
    pub fn finish(&mut self,history_exhausted:bool)->Option<LogicalPost> {
        self.take_post(history_exhausted)
    }
    fn take_post(&mut self,lower_edge_seen:bool)->Option<LogicalPost> {
        let messages=std::mem::take(&mut self.current);
        self.current_key=None;
        if messages.is_empty(){return None;}
        let mut post=assemble(&messages).into_iter().next()?;
        post.completeness=if post.kind==PostKind::Single {
            PostCompleteness{level:CompletenessLevel::Verified,reason:"single_message".into()}
        } else if self.upper_edge_seen&&lower_edge_seen {
            PostCompleteness{level:CompletenessLevel::Assumed,reason:"contiguous_boundaries_observed".into()}
        } else {
            PostCompleteness{level:CompletenessLevel::Unknown,reason:if lower_edge_seen {
                "newer_boundary_not_observed"
            } else {"older_boundary_not_observed"}.into()}
        };
        Some(post)
    }
}
