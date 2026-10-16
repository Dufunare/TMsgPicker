use crate::formatted_text::extract_text_links;
pub use crate::formatted_text::TextLink;
use crate::gateway::{Result, TelegramGateway, id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    sync::atomic::AtomicBool,
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
    #[serde(default)]
    pub links: Vec<TextLink>,
    pub metrics: InteractionMetrics,
    #[serde(default)]
    pub thumbnail_file_id: Option<i32>,
    #[serde(default)]
    pub full_file_id: Option<i32>,
    #[serde(default)]
    pub minithumbnail: Option<String>,
}
fn numeric(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
/// Choose the most appropriate size for a list thumbnail or a full-screen image.
fn photo_file(photo: &Value, largest: bool) -> Option<i32> {
    photo["sizes"].as_array()?
        .iter()
        .filter_map(|s| {
            let id = i32::try_from(numeric(&s["photo"]["id"])?).ok()?;
            let area = numeric(&s["width"])?.saturating_mul(numeric(&s["height"])?);
            (id > 0 && area > 0).then_some((id, area))
        })
        .reduce(|a, b| {
            if largest {
                if b.1 > a.1 { b } else { a }
            } else if (b.1 - 320 * 320).abs() < (a.1 - 320 * 320).abs() {
                b
            } else { a }
        })
        .map(|(id, _)| id)
}
fn td_file_id(file: &Value) -> Option<i32> {
    i32::try_from(numeric(&file["id"])?).ok().filter(|id| *id > 0)
}
fn mini_uri(value: &Value) -> Option<String> {
    let data = value["data"].as_str()?;
    (data.len() <= 16_384 && !data.is_empty()
        && data.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=')))
        .then(|| format!("data:image/jpeg;base64,{data}"))
}
pub fn normalize(v: &Value) -> Result<TelegramMessage> {
    let chat = numeric(&v["chat_id"]).ok_or("Message has no chat ID")?;
    let message = numeric(&v["id"])
        .filter(|n| *n > 0)
        .ok_or("Message has no valid ID")?;
    let content = &v["content"];
    let formatted = if content["text"]["text"].is_string() {
        &content["text"]
    } else {
        &content["caption"]
    };
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
        text: formatted["text"].as_str().unwrap_or("").into(),
        links: extract_text_links(formatted),
        // Real TDLib video thumbnails are not always JPEG; the media loader sniffs
        // the returned format and the tiny JPEG serves only as a loading fallback.
        thumbnail_file_id: match kind {
            "messagePhoto" => photo_file(&content["photo"], false),
            "messageVideo" => photo_file(&content["cover"], true)
                .or_else(|| photo_file(&content["video"]["cover"], true))
                .or_else(|| td_file_id(&content["video"]["thumbnail"]["file"])),
            "messageAnimation" => td_file_id(&content["animation"]["thumbnail"]["file"]),
            "messageDocument" => td_file_id(&content["document"]["thumbnail"]["file"]),
            "messageVideoNote" => td_file_id(&content["video_note"]["thumbnail"]["file"]),
            "messageSticker" => td_file_id(&content["sticker"]["thumbnail"]["file"]),
            _ => None,
        },
        full_file_id: match kind {
            "messagePhoto" => photo_file(&content["photo"], true),
            "messageVideo" => td_file_id(&content["video"]["video"]),
            "messageAnimation" => td_file_id(&content["animation"]["animation"]),
            _ => None,
        },
        minithumbnail: match kind {
            "messagePhoto" => mini_uri(&content["photo"]["minithumbnail"]),
            "messageVideo" => mini_uri(&content["video"]["minithumbnail"])
                .or_else(|| mini_uri(&content["cover"]["minithumbnail"])),
            "messageAnimation" => mini_uri(&content["animation"]["minithumbnail"]),
            "messageDocument" => mini_uri(&content["document"]["minithumbnail"]),
            _ => None,
        },
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="PascalCase")]
pub enum CompletenessLevel { Verified, Assumed, Truncated, Unknown }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostCompleteness {
    pub level: CompletenessLevel,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogicalPost {
    pub key: String,
    pub kind: PostKind,
    pub members: Vec<TelegramMessage>,
    pub representative: MessageKey,
    pub date: i64,
    pub text: String,
    #[serde(default)]
    pub links: Vec<TextLink>,
    pub metrics: InteractionMetrics,
    pub provenance: BTreeMap<String, String>,
    pub completeness: PostCompleteness,
}
/// Pure grouping from an arbitrary batch: without boundary evidence, Albums
/// are Unknown. Only the streaming boundary resolver may promote to Assumed.
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
            // Canonical interaction owner: earliest caption-bearing member, otherwise first.
            let primary = members.iter().find(|m| !m.text.is_empty()).unwrap_or(&members[0]);
            let album = members[0].media_album_id.is_some();
            let mut metrics = InteractionMetrics {
                views: None, forwards: None,
                reply_info: primary.metrics.reply_info,
                reactions: primary.metrics.reactions.clone(),
            };
            let mut provenance = BTreeMap::new();
            provenance.insert("reply_info".into(),primary.key.message_id.clone());
            provenance.insert("reactions".into(),primary.key.message_id.clone());
            for m in &members {
                for (field,target,value) in [
                    ("views",&mut metrics.views,m.metrics.views),
                    ("forwards",&mut metrics.forwards,m.metrics.forwards),
                ] {
                    if let Some(value)=value && target.is_none_or(|old|value>old) {
                        *target=Some(value);
                        provenance.insert(field.into(),m.key.message_id.clone());
                    }
                }
            }
            LogicalPost {
                key,
                kind: if album {
                    PostKind::Album
                } else {
                    PostKind::Single
                },
                representative: primary.key.clone(),
                date: members.iter().map(|m|m.date).max().unwrap_or(primary.date),
                text: primary.text.clone(),
                links: primary.links.clone(),
                metrics,
                provenance,
                completeness: PostCompleteness {
                    level:if album {CompletenessLevel::Unknown} else {CompletenessLevel::Verified},
                    reason:if album {"unverified_membership".into()} else {"single_message".into()},
                },
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
    ContainsContent { content_kind: String, exists: bool },
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
                .find(|r| matches!(&r.key, ReactionKey::Emoji(value) if value == e))
                .map_or(0, |r| r.count)
        }),
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
        FilterExpr::ContainsContent{content_kind,exists} => {
            let present=p.members.iter().any(|m|&m.content_kind==content_kind);
            if present==*exists {Truth::True}else{Truth::False}
        }
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
        FilterExpr::ContainsContent{content_kind,..} => {
            if !matches!(content_kind.as_str(),"text"|"photo"|"video"|"animation"|"document"|"audio"|"voice_note"|"video_note"|"sticker"|"poll") {return Err("Unsupported content type".into());}
        }
        FilterExpr::Compare{value,..} => {
            if !matches!(value,Scalar::Number(_)){return Err("Numeric comparison requires numeric value".into());}
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
#[serde(tag = "kind", rename_all = "snake_case",deny_unknown_fields)]
pub enum Scope {
    Recent { count: usize },
    Dates { from: i64, to: i64, limit_posts: Option<usize> },
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
        if self.version != 2
            || self.output_limit == 0 || self.output_limit > 1000
            || self.sort.len() > 10
        {return Err("QuerySpec V2 requires version 2 and output limit 1–1000".into());}
        let requested=match &self.scope {
            Scope::Recent{count}=>Some(*count),
            Scope::Dates{from,to,limit_posts}=>{
                if from>=to{return Err("Date range must be [from,to)".into());}
                *limit_posts
            }
        };
        if requested.is_some_and(|n|n==0||n>100_000) {
            return Err("Post count must be 1–100000".into());
        }
        validate_filter(&self.filter,0)
    }
}
pub fn query(messages: &[TelegramMessage], spec: &QuerySpec) -> Vec<LogicalPost> {
    query_posts(&assemble(messages),spec)
}
/// Ordering shared by the full-sort API and the streaming bounded Top-K index.
/// Less means the left Post ranks higher; null fields always rank last.
pub fn compare_posts(a:&LogicalPost,b:&LogicalPost,sort:&[SortSpec])->Ordering {
    for s in sort {
        let cmp=match (field_value(a,&s.field),field_value(b,&s.field)) {
            (None,None)=>Ordering::Equal,
            (None,Some(_))=>Ordering::Greater,
            (Some(_),None)=>Ordering::Less,
            (Some(x),Some(y))=>{
                let c=scalar_cmp(&x,&y).unwrap_or(Ordering::Equal);
                if s.descending {c.reverse()} else {c}
            }
        };
        if !cmp.is_eq(){return cmp;}
    }
    (
        a.date,
        id(&a.representative.chat_id).unwrap_or(0),
        id(&a.representative.message_id).unwrap_or(0),
    ).cmp(&(
        b.date,
        id(&b.representative.chat_id).unwrap_or(0),
        id(&b.representative.message_id).unwrap_or(0),
    ))
}
pub fn query_posts(input:&[LogicalPost],spec:&QuerySpec)->Vec<LogicalPost> {
    let mut posts:Vec<_>=input.iter()
        .filter(|p|evaluate(p,&spec.filter)==Truth::True)
        .cloned().collect();
    posts.sort_by(|a,b|compare_posts(a,b,&spec.sort));
    posts.truncate(spec.output_limit);
    posts
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScanStatus { Running, Complete, BudgetExceeded, Interrupted, Error }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="PascalCase")]
pub enum StopReason {
    PostLimitReached, DateLowerBoundReached, HistoryExhausted,
    UserCancelled, CursorStalled, ResourceLimitReached, GatewayFailure,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ScanCoverage {
    pub requested_posts:Option<usize>,
    pub examined_posts:usize,
    pub scope_complete:bool,
    pub date_window_exhausted:Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ScanStats {
    pub raw_messages_read:usize,
    pub posts_finalized:usize,
    pub posts_matched:usize,
    pub posts_returned:usize,
    pub pages:usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub run_id:String,
    pub status:ScanStatus,
    pub stop_reason:Option<StopReason>,
    pub detail:String,
    pub scope:Scope,
    pub coverage:ScanCoverage,
    pub stats:ScanStats,
    // Compatibility-only transport counters; prefer coverage + stats in the UI.
    pub scanned:usize,
    pub pages:usize,
    pub exact:bool,
    pub posts:Vec<LogicalPost>,
}
pub fn scan(
    gateway:&dyn TelegramGateway,
    spec:&QuerySpec,
    run_id:String,
    cancel:&AtomicBool,
    progress:impl FnMut(ScanResult),
)->Result<ScanResult> {
    crate::scan::scan(gateway,spec,run_id,cancel,progress)
}
