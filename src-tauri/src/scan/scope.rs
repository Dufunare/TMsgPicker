//! Post-level scope decisions. Never count raw Telegram messages here.
use crate::core::{LogicalPost, Scope, StopReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Skip,
    Include,
    IncludeAndStop,
    Stop,
}

pub struct ScopeController<'a> {
    scope: &'a Scope,
    examined: usize,
    window_exhausted: bool,
}
impl<'a> ScopeController<'a> {
    pub fn new(scope: &'a Scope) -> Self {
        Self {scope,examined:0,window_exhausted:false}
    }
    pub fn observe(&mut self,post:&LogicalPost)->Decision {
        match self.scope {
            Scope::Recent {count}=>{
                self.examined+=1;
                if self.examined>=*count {Decision::IncludeAndStop} else {Decision::Include}
            }
            Scope::Dates {from,to,limit_posts}=>{
                if post.date>=*to {return Decision::Skip;}
                if post.date<*from {
                    self.window_exhausted=true;
                    return Decision::Stop;
                }
                self.examined+=1;
                if limit_posts.is_some_and(|n|self.examined>=n) {
                    Decision::IncludeAndStop
                } else {Decision::Include}
            }
        }
    }
    pub fn limit_reason(&self)->StopReason {StopReason::PostLimitReached}
    pub fn lower_boundary_reason(&self)->StopReason {StopReason::DateLowerBoundReached}
    pub fn requested(&self)->Option<usize> {
        match self.scope {
            Scope::Recent{count}=>Some(*count),
            Scope::Dates{limit_posts,..}=>*limit_posts,
        }
    }
    pub fn examined(&self)->usize {self.examined}
    pub fn mark_exhausted(&mut self){self.window_exhausted=true;}
    pub fn date_window_exhausted(&self)->Option<bool> {
        match self.scope {
            Scope::Recent{..}=>None,
            Scope::Dates{..}=>Some(self.window_exhausted),
        }
    }
    pub fn remaining_hint(&self)->usize {
        match self.requested() {
            Some(n)=>n.saturating_sub(self.examined).max(1),
            None=>100,
        }
    }
}
