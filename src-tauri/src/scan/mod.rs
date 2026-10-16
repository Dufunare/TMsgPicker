//! Query coordination: history -> streaming Posts -> Post scope -> filter/rank.
pub mod accumulator;
pub mod boundary;
pub mod history;
pub mod scope;
pub mod topk;
use accumulator::PostAccumulator;
use history::{HistoryReader, Page};
use scope::{Decision, ScopeController};
use topk::TopPosts;
use crate::core::{QuerySpec,ScanResult,ScanStats,ScanCoverage,ScanStatus,StopReason};
use crate::gateway::{Result, TelegramGateway};
use std::{sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};

/// Internal resource guards. These do not change user-visible Post counts.
#[derive(Clone,Copy,Debug)]
pub struct ScanLimits {
    pub max_raw_messages:usize,
    pub max_pages:usize,
    pub max_duration:Duration,
    pub max_boundary_messages:usize,
}
impl Default for ScanLimits {
    fn default()->Self {
        Self{max_raw_messages:100_000,max_pages:2_500,
            max_duration:Duration::from_secs(300),max_boundary_messages:64}
    }
}

pub fn scan(
    gateway:&dyn TelegramGateway,
    spec:&QuerySpec,
    run_id:String,
    cancel:&AtomicBool,
    progress:impl FnMut(ScanResult),
)->Result<ScanResult> {
    scan_with_limits(gateway,spec,run_id,cancel,ScanLimits::default(),progress)
}

pub fn scan_with_limits(
    gateway:&dyn TelegramGateway,
    spec:&QuerySpec,
    run_id:String,
    cancel:&AtomicBool,
    limits:ScanLimits,
    mut progress:impl FnMut(ScanResult),
)->Result<ScanResult> {
    spec.validate()?;
    let mut reader=HistoryReader::new(gateway,&spec.chat_id,&spec.scope,cancel)?;
    let mut assembler=PostAccumulator::new(reader.from_head());
    let mut scope=ScopeController::new(&spec.scope);
    let mut ranking=TopPosts::new(&spec.sort,spec.output_limit);
    let mut finalized=0usize;
    // Every exit path assigns all three exactly once; no provisional scan
    // status is mistaken for a terminal value.
    let status:ScanStatus;
    let reason:Option<StopReason>;
    let detail:String;
    let started=Instant::now();
    let mut last_progress=None::<Instant>;
    let to_bound=match spec.scope { crate::core::Scope::Dates{to,..}=>Some(to), _=>None };
    let mut processed_raw=0usize;
    let mut boundary_extra=0usize;

    'scan: loop {
        if cancel.load(Ordering::Relaxed){
            status=ScanStatus::Interrupted;reason=Some(StopReason::UserCancelled);
            detail="Cancelled by user".into();break;
        }
        if reader.pages>=limits.max_pages || started.elapsed()>=limits.max_duration {
            status=ScanStatus::BudgetExceeded;reason=Some(StopReason::ResourceLimitReached);
            detail="Page or duration safety limit reached".into();break;
        }
        let raw_limit=limits.max_raw_messages.saturating_add(limits.max_boundary_messages);
        if processed_raw>=raw_limit {
            status=ScanStatus::BudgetExceeded;reason=Some(StopReason::ResourceLimitReached);
            detail="Raw-message safety limit reached while resolving Post boundary".into();break;
        }
        let hint=if assembler.is_open(){scope.remaining_hint().max(12)}else{scope.remaining_hint()};
        let page=match reader.next_page(hint,to_bound,cancel) {
            Ok(page)=>page,
            Err(err)=>{
                if cancel.load(Ordering::Relaxed) {
                    status=ScanStatus::Interrupted;reason=Some(StopReason::UserCancelled);
                } else {
                    status=ScanStatus::Error;reason=Some(StopReason::GatewayFailure);
                }
                detail=err;break;
            }
        };
        match page {
            Page::Stalled=>{
                status=ScanStatus::Interrupted;reason=Some(StopReason::CursorStalled);
                detail="History cursor made no progress after retries".into();
                break;
            }
            Page::Exhausted=>{
                if let Some(post)=assembler.finish(true) {
                    finalized+=1;
                    match scope.observe(&post){
                        Decision::Skip=>{}
                        Decision::Include=>ranking.consider(post,&spec.filter),
                        Decision::IncludeAndStop=>{
                            ranking.consider(post,&spec.filter);status=ScanStatus::Complete;
                            reason=Some(StopReason::PostLimitReached);
                            detail="Requested Post count reached".into();break 'scan;
                        }
                        Decision::Stop=>{
                            status=ScanStatus::Complete;reason=Some(StopReason::DateLowerBoundReached);
                            detail="Date lower boundary reached".into();break 'scan;
                        }
                    }
                }
                scope.mark_exhausted();
                status=ScanStatus::Complete;reason=Some(StopReason::HistoryExhausted);
                detail="History exhausted".into();break;
            }
            Page::Items(messages)=>{
                if reader.from_head()&&!assembler.is_open(){assembler.mark_head();}
                for message in messages {
                    // A strict, independent resource ceiling. We may spend a small
                    // additional allowance only while an Album boundary is open.
                    if processed_raw>=limits.max_raw_messages {
                        if !assembler.is_open() || boundary_extra>=limits.max_boundary_messages {
                            status=ScanStatus::BudgetExceeded;reason=Some(StopReason::ResourceLimitReached);
                            detail="Raw-message safety limit reached".into();break 'scan;
                        }
                        boundary_extra+=1;
                    }
                    processed_raw+=1;
                    for post in assembler.push(message) {
                        finalized+=1;
                        match scope.observe(&post){
                            Decision::Skip=>{}
                            Decision::Include=>ranking.consider(post,&spec.filter),
                            Decision::IncludeAndStop=>{
                                ranking.consider(post,&spec.filter);status=ScanStatus::Complete;
                                reason=Some(StopReason::PostLimitReached);
                                detail="Requested Post count reached".into();break 'scan;
                            }
                            Decision::Stop=>{
                                status=ScanStatus::Complete;reason=Some(StopReason::DateLowerBoundReached);
                                detail="Date lower boundary reached".into();break 'scan;
                            }
                        }
                    }
                }
            }
        }
        if last_progress.is_none_or(|at|at.elapsed()>=Duration::from_millis(500)) {
            last_progress=Some(Instant::now());
            progress(snapshot(&run_id,spec,ScanStatus::Running,None,"Scanning; ranking is provisional",
                &reader,&scope,&ranking,finalized,false,processed_raw));
        }
    }
    // Only offer a partial open group when the scan ended abnormally.
    // It stays Unknown (unless it is a single) and cannot be forwarded without
    // the explicit incomplete-Post confirmation.
    if status!=ScanStatus::Complete && let Some(post)=assembler.finish(false) {
        finalized+=1;
        if matches!(scope.observe(&post),Decision::Include|Decision::IncludeAndStop){
            ranking.consider(post,&spec.filter);
        }
    }
    let covered=status==ScanStatus::Complete;
    Ok(snapshot(&run_id,spec,status,reason,&detail,&reader,&scope,&ranking,finalized,covered,processed_raw))
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    run_id:&str,spec:&QuerySpec,status:ScanStatus,reason:Option<StopReason>,detail:&str,
    reader:&HistoryReader<'_>,scope:&ScopeController<'_>,ranking:&TopPosts,
    finalized:usize,complete:bool,processed_raw:usize,
)->ScanResult {
    // Keep the IPC and WebView lightweight while scanning. Only the final
    // result transfers the complete output_limit (up to 1000) to the UI.
    const PROGRESS_POSTS:usize=40;
    let posts=ranking.snapshot(if status==ScanStatus::Running {
        PROGRESS_POSTS.min(spec.output_limit)
    } else {spec.output_limit});
    let returned=posts.len();
    ScanResult {
        run_id:run_id.into(),status,stop_reason:reason,detail:detail.into(),
        scope:spec.scope.clone(),
        coverage:ScanCoverage {requested_posts:scope.requested(),examined_posts:scope.examined(),
            scope_complete:complete,date_window_exhausted:scope.date_window_exhausted()},
        stats:ScanStats {raw_messages_read:reader.raw_messages_read,posts_finalized:finalized,
            posts_matched:ranking.matched(),posts_returned:returned,pages:reader.pages},
        // Legacy counters stay temporarily for existing UI integrations; no
        // completeness decision is made using this alias.
        scanned:processed_raw,pages:reader.pages,exact:complete,
        posts,
    }
}
