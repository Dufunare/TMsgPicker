//! Bounded streaming top-K. Unlike full sorting at every progress event, this
//! holds at most output_limit Posts, even when a query scans 100k messages.
use crate::core::{compare_posts,evaluate,FilterExpr,LogicalPost,SortSpec,Truth};
use std::{cmp::Ordering,collections::BinaryHeap,sync::Arc};

struct RankedPost {
    post:LogicalPost,
    sort:Arc<[SortSpec]>,
}
impl Ord for RankedPost {
    fn cmp(&self,other:&Self)->Ordering {
        // BinaryHeap.peek() is the *worst* retained Post.
        compare_posts(&self.post,&other.post,&self.sort)
    }
}
impl PartialOrd for RankedPost {
    fn partial_cmp(&self,other:&Self)->Option<Ordering>{Some(self.cmp(other))}
}
impl PartialEq for RankedPost {
    fn eq(&self,other:&Self)->bool{self.cmp(other)==Ordering::Equal}
}
impl Eq for RankedPost {}

pub struct TopPosts {
    heap:BinaryHeap<RankedPost>,
    sort:Arc<[SortSpec]>,
    limit:usize,
    matched:usize,
}
impl TopPosts {
    pub fn new(sort:&[SortSpec],limit:usize)->Self {
        Self{heap:BinaryHeap::new(),sort:sort.into(),limit,matched:0}
    }
    /// Each finalized Post is evaluated once. Non-matching and out-of-top-K
    /// Posts are dropped immediately instead of retained until scan completion.
    pub fn consider(&mut self,post:LogicalPost,filter:&FilterExpr) {
        if evaluate(&post,filter)!=Truth::True{return;}
        self.matched+=1;
        if self.limit==0{return;}
        let entry=RankedPost{post,sort:self.sort.clone()};
        if self.heap.len()<self.limit {
            self.heap.push(entry);
        } else if self.heap.peek().is_some_and(|worst|entry.cmp(worst)==Ordering::Less) {
            self.heap.pop();
            self.heap.push(entry);
        }
    }
    pub fn matched(&self)->usize{self.matched}
    pub fn len(&self)->usize{self.heap.len()}
    /// Clone only the small, retained ranking. Running updates can send fewer
    /// Posts than the final snapshot without changing matched/count semantics.
    pub fn snapshot(&self,max_items:usize)->Vec<LogicalPost> {
        let mut refs:Vec<_>=self.heap.iter().collect();
        refs.sort_unstable_by(|a,b|a.cmp(b));
        refs.into_iter().take(max_items).map(|item|item.post.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{assemble,InteractionMetrics,MessageKey,TelegramMessage,SortSpec,Field};
    fn post(id:i64,views:Option<i64>,reactions:Option<Vec<crate::core::Reaction>>)->LogicalPost {
        let m=TelegramMessage{
            key:MessageKey{chat_id:"-1001".into(),message_id:id.to_string()},
            date:id,media_album_id:None,content_kind:"text".into(),
            text:"Synthetic".into(),links:vec![],thumbnail_file_id:None,
            full_file_id:None,minithumbnail:None,
            metrics:InteractionMetrics{views,forwards:None,reply_info:None,reactions},
        };
        assemble(&[m]).pop().unwrap()
    }
    #[test]
    fn topk_order_matches_reference_full_sort_and_counts_all_hits() {
        use crate::core::{query_posts,QuerySpec,Scope,FilterExpr};
        let base:Vec<_>=(1..=100).map(|n|post(n,if n%7==0{None}else{Some(n%17)},None)).collect();
        for descending in [true,false] {
            let spec=QuerySpec{version:2,chat_id:"-1001".into(),scope:Scope::Recent{count:100},
                sort:vec![SortSpec{field:Field::Views,descending}],output_limit:9,
                filter:FilterExpr::And{items:vec![]}};
            let expected=query_posts(&base,&spec);
            let mut top=TopPosts::new(&spec.sort,spec.output_limit);
            for item in base.iter().cloned(){top.consider(item,&spec.filter);}
            assert_eq!(top.matched(),base.len());
            assert_eq!(top.len(),spec.output_limit);
            assert_eq!(top.snapshot(9).iter().map(|p|&p.key).collect::<Vec<_>>(),
                expected.iter().map(|p|&p.key).collect::<Vec<_>>());
            assert_eq!(top.snapshot(3).len(),3);
        }
    }
    #[test]
    fn no_match_retained_and_filter_tristate_matches_reference() {
        use crate::core::{FilterExpr,CompareOp,Scalar};
        let filter=FilterExpr::Compare{field:Field::Views,op:CompareOp::Gt,value:Scalar::Number(10)};
        let mut top=TopPosts::new(&[SortSpec{field:Field::Date,descending:true}],2);
        for i in 0..20 {top.consider(post(i,if i%3==0{None}else{Some(i)},None),&filter);}
        assert_eq!(top.matched(),6);
        assert_eq!(top.snapshot(2).len(),2);
        assert_eq!(top.snapshot(2).iter().map(|p|p.date).collect::<Vec<_>>(),vec![19,17]);
    }
}
