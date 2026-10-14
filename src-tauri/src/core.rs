use crate::gateway::{Result, TelegramGateway, id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ReactionKey {
    Emoji(String),
    CustomEmoji(String),
    Paid,
    Unknown(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reaction {
    pub key: ReactionKey,
    pub count: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InteractionMetrics {
    pub views: Option<i64>,
    pub forwards: Option<i64>,
    pub reply_info: Option<i64>,
    pub reactions: Option<Vec<Reaction>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageKey {
    pub chat_id: String,
    pub message_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub key: MessageKey,
    pub date: i64,
    pub media_album_id: Option<String>,
    pub content_kind: String,
    pub text: String,
    pub metrics: InteractionMetrics,
}
fn numeric(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
pub fn normalize(v: &Value) -> Result<TelegramMessage> {
    let chat = numeric(&v["chat_id"]).ok_or("Message has no chat ID")?;
    let message = numeric(&v["id"])
        .filter(|n| *n > 0)
        .ok_or("Message has no valid ID")?;
    let content = &v["content"];
    let kind = content["@type"].as_str().unwrap_or("unknown");
    let info = &v["interaction_info"];
    let reactions = info["reactions"]["reactions"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|r| {
                    let t = &r["type"];
                    let key = match t["@type"].as_str().unwrap_or("unknown") {
                        "reactionTypeEmoji" => {
                            ReactionKey::Emoji(t["emoji"].as_str().unwrap_or("?").into())
                        }
                        "reactionTypeCustomEmoji" => ReactionKey::CustomEmoji(
                            t["custom_emoji_id"]
                                .as_str()
                                .map(str::to_owned)
                                .or_else(|| numeric(&t["custom_emoji_id"]).map(|n| n.to_string()))
                                .unwrap_or_else(|| "?".into()),
                        ),
                        "reactionTypePaid" => ReactionKey::Paid,
                        other => ReactionKey::Unknown(other.into()),
                    };
                    Ok(Reaction {
                        key,
                        count: numeric(&r["total_count"])
                            .filter(|n| *n >= 0)
                            .ok_or("Invalid reaction count")?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    Ok(TelegramMessage {
        key: MessageKey {
            chat_id: chat.to_string(),
            message_id: message.to_string(),
        },
        date: numeric(&v["date"]).ok_or("Message has no date")?,
        media_album_id: numeric(&v["media_album_id"])
            .filter(|n| *n != 0)
            .map(|n| n.to_string()),
        content_kind: kind.strip_prefix("message").unwrap_or(kind).to_lowercase(),
        text: content["text"]["text"]
            .as_str()
            .or_else(|| content["caption"]["text"].as_str())
            .unwrap_or("")
            .into(),
        metrics: InteractionMetrics {
            views: numeric(&info["view_count"]),
            forwards: numeric(&info["forward_count"]),
            reply_info: numeric(&info["reply_info"]["reply_count"]),
            reactions,
        },
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PostKind {
    Single,
    Album,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogicalPost {
    pub key: String,
    pub kind: PostKind,
    pub members: Vec<TelegramMessage>,
    pub representative: MessageKey,
    pub date: i64,
    pub text: String,
    pub metrics: InteractionMetrics,
    pub provenance: BTreeMap<String, String>,
    pub partial: bool,
}
/// Every album is conservatively Partial: TDLib doesn't supply a total member count.
pub fn assemble(messages: &[TelegramMessage]) -> Vec<LogicalPost> {
    let mut groups: BTreeMap<String, Vec<TelegramMessage>> = BTreeMap::new();
    for m in messages {
        let key = format!(
            "{}:{}",
            m.key.chat_id,
            m.media_album_id
                .as_ref()
                .map(|a| format!("a{a}"))
                .unwrap_or_else(|| format!("m{}", m.key.message_id))
        );
        groups.entry(key).or_default().push(m.clone());
    }
    groups
        .into_iter()
        .map(|(key, mut members)| {
            members.sort_by_key(|m| (m.date, id(&m.key.message_id).unwrap_or(0)));
            members.dedup_by(|a, b| a.key == b.key);
            let first = &members[0];
            let album = first.media_album_id.is_some();
            let mut metrics = InteractionMetrics::default();
            let mut provenance = BTreeMap::new();
            for m in &members {
                for (field, target, value) in [
                    ("views", &mut metrics.views, m.metrics.views),
                    ("forwards", &mut metrics.forwards, m.metrics.forwards),
                    ("reply_info", &mut metrics.reply_info, m.metrics.reply_info),
                ] {
                    if target.is_none() && value.is_some() {
                        *target = value;
                        provenance.insert(field.into(), m.key.message_id.clone());
                    }
                }
                if metrics.reactions.is_none() && m.metrics.reactions.is_some() {
                    metrics.reactions = m.metrics.reactions.clone();
                    provenance.insert("reactions".into(), m.key.message_id.clone());
                }
            }
            LogicalPost {
                key,
                kind: if album {
                    PostKind::Album
                } else {
                    PostKind::Single
                },
                representative: first.key.clone(),
                date: first.date,
                text: members
                    .iter()
                    .find(|m| !m.text.is_empty())
                    .map(|m| m.text.clone())
                    .unwrap_or_default(),
                metrics,
                provenance,
                partial: album,
                members,
            }
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "emoji", rename_all = "snake_case")]
pub enum Field {
    Views,
    Forwards,
    Comments,
    ReplyInfo,
    ReactionTotal,
    Reaction(String),
    PostKind,
    MediaAlbumId,
    ContentKind,
    Date,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Scalar {
    Number(i64),
    Text(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterExpr {
    Compare {
        field: Field,
        op: CompareOp,
        value: Scalar,
    },
    IsNull {
        field: Field,
    },
    IsNotNull {
        field: Field,
    },
    And {
        items: Vec<FilterExpr>,
    },
    Or {
        items: Vec<FilterExpr>,
    },
    Not {
        item: Box<FilterExpr>,
    },
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Truth {
    True,
    False,
    Unknown,
}
impl Truth {
    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}
pub fn field_value(p: &LogicalPost, f: &Field) -> Option<Scalar> {
    let m = &p.metrics;
    let n = match f {
        Field::Views => m.views,
        Field::Forwards => m.forwards,
        Field::Comments | Field::ReplyInfo => m.reply_info,
        Field::Date => Some(p.date),
        Field::ReactionTotal => m
            .reactions
            .as_ref()
            .map(|rs| rs.iter().map(|r| r.count).sum()),
        // An absent emoji in an available reactions object is known zero; missing object is Unknown.
        Field::Reaction(e) => m.reactions.as_ref().map(|rs| {
            rs.iter()
                .find(|r| r.key == ReactionKey::Emoji(e.clone()))
                .map_or(0, |r| r.count)
        }),
        Field::PostKind => {
            return Some(Scalar::Text(
                if p.kind == PostKind::Album {
                    "album"
                } else {
                    "single"
                }
                .into(),
            ));
        }
        Field::ContentKind => return Some(Scalar::Text(p.members[0].content_kind.clone())),
        Field::MediaAlbumId => return p.members[0].media_album_id.clone().map(Scalar::Text),
    };
    n.map(Scalar::Number)
}
fn scalar_cmp(a: &Scalar, b: &Scalar) -> Option<Ordering> {
    match (a, b) {
        (Scalar::Number(a), Scalar::Number(b)) => Some(a.cmp(b)),
        (Scalar::Text(a), Scalar::Text(b)) => Some(a.cmp(b)),
        _ => None,
    }
}
pub fn evaluate(p: &LogicalPost, f: &FilterExpr) -> Truth {
    match f {
        FilterExpr::Compare { field, op, value } => {
            let Some(order) = field_value(p, field).and_then(|a| scalar_cmp(&a, value)) else {
                return Truth::Unknown;
            };
            let yes = match op {
                CompareOp::Eq => order.is_eq(),
                CompareOp::Ne => !order.is_eq(),
                CompareOp::Gt => order.is_gt(),
                CompareOp::Ge => !order.is_lt(),
                CompareOp::Lt => order.is_lt(),
                CompareOp::Le => !order.is_gt(),
            };
            if yes { Truth::True } else { Truth::False }
        }
        FilterExpr::IsNull { field } => {
            if field_value(p, field).is_none() {
                Truth::True
            } else {
                Truth::False
            }
        }
        FilterExpr::IsNotNull { field } => {
            if field_value(p, field).is_some() {
                Truth::True
            } else {
                Truth::False
            }
        }
        FilterExpr::Not { item } => evaluate(p, item).not(),
        FilterExpr::And { items } => {
            let mut unknown = false;
            for i in items {
                match evaluate(p, i) {
                    Truth::False => return Truth::False,
                    Truth::Unknown => unknown = true,
                    _ => {}
                }
            }
            if unknown { Truth::Unknown } else { Truth::True }
        }
        FilterExpr::Or { items } => {
            let mut unknown = false;
            for i in items {
                match evaluate(p, i) {
                    Truth::True => return Truth::True,
                    Truth::Unknown => unknown = true,
                    _ => {}
                }
            }
            if unknown {
                Truth::Unknown
            } else {
                Truth::False
            }
        }
    }
}
fn validate_filter(f: &FilterExpr, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err("Filter nesting exceeds 16".into());
    }
    match f {
        FilterExpr::Compare { field, value, .. } => {
            let text = matches!(
                field,
                Field::PostKind | Field::ContentKind | Field::MediaAlbumId
            );
            if text != matches!(value, Scalar::Text(_)) {
                return Err("Filter field/value type mismatch".into());
            }
            if matches!(field, Field::PostKind)
                && !matches!(value, Scalar::Text(s) if s == "single" || s == "album")
            {
                return Err("post_kind must be single or album".into());
            }
        }
        FilterExpr::And { items } | FilterExpr::Or { items } => {
            if items.len() > 100 {
                return Err("Too many conditions".into());
            }
            for i in items {
                validate_filter(i, depth + 1)?;
            }
        }
        FilterExpr::Not { item } => validate_filter(item, depth + 1)?,
        _ => {}
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Recent { count: usize },
    Dates { from: i64, to: i64, budget: usize },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SortSpec {
    pub field: Field,
    pub descending: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuerySpec {
    pub version: u32,
    pub chat_id: String,
    pub scope: Scope,
    pub filter: FilterExpr,
    pub sort: Vec<SortSpec>,
    pub output_limit: usize,
}
impl QuerySpec {
    pub fn validate(&self) -> Result<()> {
        id(&self.chat_id)?;
        if self.version != 1
            || self.output_limit == 0
            || self.output_limit > 1000
            || self.sort.len() > 10
        {
            return Err("Invalid query version or output limit (1–1000)".into());
        }
        let budget = match self.scope {
            Scope::Recent { count } => count,
            Scope::Dates { from, to, budget } => {
                if from >= to {
                    return Err("Date range must be [from,to)".into());
                }
                budget
            }
        };
        if budget == 0 || budget > 100_000 {
            return Err("Scan budget must be 1–100000".into());
        }
        validate_filter(&self.filter, 0)
    }
}
pub fn query(messages: &[TelegramMessage], spec: &QuerySpec) -> Vec<LogicalPost> {
    let mut posts: Vec<_> = assemble(messages)
        .into_iter()
        .filter(|p| evaluate(p, &spec.filter) == Truth::True)
        .collect();
    posts.sort_by(|a, b| {
        for s in &spec.sort {
            let cmp = match (field_value(a, &s.field), field_value(b, &s.field)) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(x), Some(y)) => {
                    let c = scalar_cmp(&x, &y).unwrap_or(Ordering::Equal);
                    if s.descending { c.reverse() } else { c }
                }
            };
            if !cmp.is_eq() {
                return cmp;
            }
        }
        (
            a.date,
            id(&a.representative.chat_id).unwrap_or(0),
            id(&a.representative.message_id).unwrap_or(0),
        )
            .cmp(&(
                b.date,
                id(&b.representative.chat_id).unwrap_or(0),
                id(&b.representative.message_id).unwrap_or(0),
            ))
    });
    posts.truncate(spec.output_limit);
    posts
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ScanStatus {
    Scanning,
    Partial,
    Complete,
    Cancelled,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub run_id: String,
    pub status: ScanStatus,
    pub scanned: usize,
    pub pages: usize,
    pub exact: bool,
    pub detail: String,
    pub posts: Vec<LogicalPost>,
}
pub fn scan(
    gateway: &dyn TelegramGateway,
    spec: &QuerySpec,
    run_id: String,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ScanResult),
) -> Result<ScanResult> {
    spec.validate()?;
    let chat = id(&spec.chat_id)?;
    let budget = match spec.scope {
        Scope::Recent { count } => count,
        Scope::Dates { budget, .. } => budget,
    };
    let mut seen = HashSet::new();
    let mut collected = Vec::new();
    let mut cursor = 0_i64;
    let mut pages = 0;
    let mut scanned = 0;
    let mut status = ScanStatus::Partial;
    let mut detail = "Scan budget reached".to_string();
    // Rebuilding and serializing all candidates on every page is quadratic for large scans.
    let mut last_progress: Option<Instant> = None;
    // TDLib may temporarily return only the inclusive cursor while loading older history.
    let mut stagnant_pages = 0_u8;
    while scanned < budget {
        if cancel.load(AtomicOrdering::Relaxed) {
            status = ScanStatus::Cancelled;
            detail = "Cancelled by user".into();
            break;
        }
        // Reserve a slot for the inclusive cursor so a remaining budget of 1 can progress.
        let page_limit = (budget - scanned + usize::from(cursor != 0)).min(100);
        let response=match gateway.request(json!({"@type":"getChatHistory","chat_id":chat,"from_message_id":cursor,"offset":0,"limit":page_limit,"only_local":false}),cancel) {
            Ok(v)=>v,Err(e)=>{status=if cancel.load(AtomicOrdering::Relaxed) {ScanStatus::Cancelled} else {ScanStatus::Error};detail=e;break;}
        };
        pages += 1;
        let Some(list) = response["messages"].as_array() else {
            status = ScanStatus::Error;
            detail = "Invalid history response".into();
            break;
        };
        if list.is_empty() {
            status = ScanStatus::Complete;
            detail = "History exhausted".into();
            break;
        }
        let mut next = cursor;
        let mut reached = false;
        for raw in list.iter().filter(|v| !v.is_null()) {
            let m = match normalize(raw) {
                Ok(m) => m,
                Err(e) => {
                    status = ScanStatus::Error;
                    detail = e;
                    reached = true;
                    break;
                }
            };
            if m.key.chat_id != spec.chat_id {
                status = ScanStatus::Error;
                detail = "History returned another chat".into();
                reached = true;
                break;
            }
            let mid = id(&m.key.message_id)?;
            if cursor != 0 && mid >= cursor {
                continue;
            }
            if next == 0 || mid < next {
                next = mid;
            }
            if !seen.insert(m.key.clone()) {
                continue;
            }
            scanned += 1;
            if let Scope::Dates { from, to, .. } = spec.scope {
                if m.date < from {
                    status = ScanStatus::Complete;
                    detail = "Date lower boundary reached".into();
                    reached = true;
                    break;
                }
                if m.date >= to {
                    if scanned >= budget {
                        break;
                    }
                    continue;
                }
            }
            collected.push(m);
            if scanned >= budget {
                if matches!(spec.scope, Scope::Recent { .. }) {
                    status = ScanStatus::Complete;
                    detail = "Recent raw-message scope complete".into();
                }
                break;
            }
        }
        if reached || scanned >= budget {
            break;
        }
        if next == 0 || (cursor != 0 && next >= cursor) {
            // A short TDLib history page can contain just the inclusive cursor.
            // Retry briefly rather than declaring the scan incomplete immediately.
            // Remain bounded if history never advances.
            if cursor != 0 && stagnant_pages < 2 {
                stagnant_pages += 1;
                std::thread::sleep(Duration::from_millis(120));
                continue;
            }
            status = ScanStatus::Partial;
            detail = "History cursor made no progress after retries".into();
            break;
        }
        stagnant_pages = 0;
        cursor = next;
        // Publish promptly after the first page, then at most twice per second.
        // The final result is always sent by the caller regardless of throttling.
        if last_progress.is_none_or(|at| at.elapsed() >= Duration::from_millis(500)) {
            last_progress = Some(Instant::now());
            progress(ScanResult {
                run_id: run_id.clone(),
                status: ScanStatus::Scanning,
                scanned,
                pages,
                exact: false,
                detail: "Scanning; ranking is provisional".into(),
                posts: query(&collected, spec),
            });
        }
    }
    if cancel.load(AtomicOrdering::Relaxed) {
        status = ScanStatus::Cancelled;
        detail = "Cancelled by user".into();
    }
    let posts = query(&collected, spec);
    let exact = status == ScanStatus::Complete && !assemble(&collected).iter().any(|p| p.partial);
    if status == ScanStatus::Complete && !exact {
        status = ScanStatus::Partial;
        detail.push_str("; album membership cannot be certified complete");
    }
    Ok(ScanResult {
        run_id,
        status,
        scanned,
        pages,
        exact,
        detail,
        posts,
    })
}
