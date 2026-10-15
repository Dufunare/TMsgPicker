use crate::{
    core::MessageKey,
    gateway::{Result, SessionLifecycle, TelegramGateway, id},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::atomic::AtomicBool,
};

#[derive(Deserialize)]
pub struct ProxyConfig {
    pub server: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}
#[derive(Deserialize)]
pub struct ConnectConfig {
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    pub passphrase: String,
    pub proxy: Option<ProxyConfig>,
}
pub struct AuthService;
#[derive(Default)]
pub struct LogoutProgress {
    pub confirmed: bool,
    pub closed: bool,
    pub recorded: bool,
}
impl LogoutProgress {
    /// Preserve progress for retries. No local cleanup before positive logout + Closed.
    pub fn execute(
        &mut self,
        g: Option<&dyn SessionLifecycle>,
        record_confirmation: impl FnOnce() -> Result<()>,
        cleanup: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if !self.confirmed {
            let client = g.ok_or("Restore account before logout")?;
            if !client.logout_in_progress() {
                match client.request(json!({"@type":"logOut"}), &AtomicBool::new(false)) {
                    Ok(reply) if reply["@type"] == "ok" => self.confirmed = true,
                    Ok(_) => {
                        return Err(
                            "Telegram logout was not confirmed; registration retained".into()
                        );
                    }
                    Err(error) if !client.logout_in_progress() => return Err(error),
                    Err(_) => {} // Submitted but outcome unknown: observe this same client.
                }
            }
        }
        self.observe_completion(g, record_confirmation)?;
        cleanup()
    }
    /// Observe an already submitted logout, including during ordinary safe exit.
    /// This method never submits logOut or close.
    pub fn observe_completion(
        &mut self,
        g: Option<&dyn SessionLifecycle>,
        record_confirmation: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if !self.closed {
            g.ok_or("Client unavailable before confirmed Closed")?
                .wait_logout_closed()?;
            if !self.confirmed && !g.is_some_and(|g| g.logout_acknowledged()) {
                return Err("Client closed without verified logout; registration retained".into());
            }
            self.confirmed = true;
            self.closed = true;
        }
        // Persist completion only after the actual Closed update, never merely ok.
        if !self.recorded {
            record_confirmation()?;
            self.recorded = true;
        }
        Ok(())
    }
}
impl AuthService {
    /// One UI submission, with TDLib's code -> optional password states enforced.
    pub fn submit_login(
        g: &dyn TelegramGateway,
        expected_state: &str,
        code: String,
        password: String,
    ) -> Result<()> {
        let cancel = AtomicBool::new(false);
        let state = g.request(json!({"@type":"getAuthorizationState"}), &cancel)?;
        if state["@type"].as_str() != Some(expected_state) {
            return Err("Authorization step changed; review the current login form".into());
        }
        match expected_state {
            "authorizationStateWaitPassword" => {
                if password.is_empty() {
                    return Err("请输入 Telegram 两步验证密码".into());
                }
                return Self::submit(g, expected_state, password);
            }
            "authorizationStateWaitCode" => {
                if code.trim().is_empty() {
                    return Err("请输入 Telegram 验证码".into());
                }
                Self::submit(g, expected_state, code)?;
            }
            _ => return Err("This step does not accept code/password submission".into()),
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let state = g.request(json!({"@type":"getAuthorizationState"}), &cancel)?;
            match state["@type"].as_str() {
                Some("authorizationStateWaitCode") if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50))
                }
                Some("authorizationStateWaitCode") => {
                    return Err("验证码已提交，仍在等待授权状态更新，请等待后重试".into());
                }
                Some("authorizationStateWaitPassword") if !password.is_empty() => {
                    return Self::submit(g, "authorizationStateWaitPassword", password);
                }
                Some(
                    "authorizationStateWaitPassword"
                    | "authorizationStateReady"
                    | "authorizationStateWaitEmailAddress"
                    | "authorizationStateWaitEmailCode"
                    | "authorizationStateWaitOtherDeviceConfirmation"
                    | "authorizationStateWaitRegistration",
                ) => return Ok(()),
                _ => return Err("Telegram returned an unexpected authorization state".into()),
            }
        }
    }
    pub fn end_session(g: &dyn SessionLifecycle, logout: bool) -> Result<()> {
        if logout {
            g.request(json!({"@type":"logOut"}), &AtomicBool::new(false))?;
        } else {
            g.begin_close();
        }
        // logOut advances TDLib's own closing states; never race it with close.
        g.wait_closed()
    }
    pub fn initialize(g: &dyn TelegramGateway, c: ConnectConfig, root: &Path) -> Result<()> {
        let cancel = AtomicBool::new(false);
        let api_id = c
            .api_id
            .or_else(|| std::env::var("TMSGPICKER_API_ID").ok()?.parse().ok())
            .filter(|n| *n > 0)
            .ok_or("Enter API ID or set TMSGPICKER_API_ID")?;
        let api_hash = c
            .api_hash
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("TMSGPICKER_API_HASH").ok())
            .filter(|s| !s.is_empty())
            .ok_or("Enter API hash or set TMSGPICKER_API_HASH")?;
        if c.passphrase.is_empty() {
            return Err(
                "Enter the existing database passphrase (or choose one for the first login)".into(),
            );
        }
        std::fs::create_dir_all(root.join("db"))
            .map_err(|_| "Cannot create app database directory")?;
        std::fs::create_dir_all(root.join("files"))
            .map_err(|_| "Cannot create app files directory")?;
        // Pre-authentication is not pre-initialization: TDLib queues proxy requests
        // until setTdlibParameters. Send the proxy first, then parameters, then await both.
        let proxy_request = if let Some(p) = c.proxy {
            if p.server.trim().is_empty() || p.port == 0 {
                return Err("Invalid SOCKS5 host/port".into());
            }
            json!({"@type":"addProxy","proxy":{"@type":"proxy","server":p.server,"port":p.port,"type":{"@type":"proxyTypeSocks5","username":p.username,"password":p.password}},"enable":true,"comment":"TMsgPicker"})
        } else {
            json!({"@type":"disableProxy"})
        };
        let parameters = json!({"@type":"setTdlibParameters","use_test_dc":false,"database_directory":root.join("db").to_string_lossy(),"files_directory":root.join("files").to_string_lossy(),"database_encryption_key":STANDARD.encode(c.passphrase.as_bytes()),"use_file_database":true,"use_chat_info_database":true,"use_message_database":true,"use_secret_chats":false,"api_id":api_id,"api_hash":api_hash,"system_language_code":"zh-CN","device_model":"TMsgPicker Desktop","system_version":"Windows","application_version":"0.1.0"});
        let responses = g
            .request_batch(vec![proxy_request, parameters], &cancel)
            .map_err(|e| format!("TDLib bootstrap failed (proxy queued before parameters): {e}"))?;
        if responses.len() != 2
            || responses[1]["@type"] != "ok"
            || !matches!(responses[0]["@type"].as_str(), Some("addedProxy" | "ok"))
        {
            return Err("TDLib returned an unexpected proxy/initialization response".into());
        }
        Ok(())
    }
    pub fn submit(g: &dyn TelegramGateway, state: &str, value: String) -> Result<()> {
        let body = match state {
            "authorizationStateWaitPhoneNumber" => {
                json!({"@type":"setAuthenticationPhoneNumber","phone_number":value,"settings":null})
            }
            "authorizationStateWaitCode" => json!({"@type":"checkAuthenticationCode","code":value}),
            "authorizationStateWaitPassword" => {
                json!({"@type":"checkAuthenticationPassword","password":value})
            }
            "authorizationStateWaitEmailAddress" => {
                json!({"@type":"setAuthenticationEmailAddress","email_address":value})
            }
            "authorizationStateWaitEmailCode" => {
                json!({"@type":"checkAuthenticationEmailCode","code":{"@type":"emailAddressAuthenticationCode","code":value}})
            }
            _ => {
                return Err(format!(
                    "Authorization step {state} requires Telegram official client or restarting connection"
                ));
            }
        };
        g.request(body, &AtomicBool::new(false))?;
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub photo_file_id: Option<i32>,
    #[serde(default)]
    pub photo_big_file_id: Option<i32>,
    #[serde(default)]
    pub photo_minithumbnail: Option<String>,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatListScope {
    #[default]
    All,
    Main,
    Archive,
}

pub struct ChatService;
impl ChatService {
    pub fn describe(v: Value) -> Result<Chat> {
        let archived = v["positions"]
            .as_array()
            .is_some_and(|positions| positions.iter().any(|position| {
                position["list"]["@type"] == "chatListArchive"
            }));
        Ok(Chat {
            id: v["id"].as_i64().ok_or("Invalid chat ID")?.to_string(),
            title: v["title"].as_str().unwrap_or("Untitled").into(),
            kind: v["type"]["@type"].as_str().unwrap_or("unknown").into(),
            archived,
            photo_file_id: v["photo"]["small"]["id"].as_i64().and_then(|id| i32::try_from(id).ok()).filter(|id| *id > 0),
            photo_big_file_id: v["photo"]["big"]["id"].as_i64().and_then(|id| i32::try_from(id).ok()).filter(|id| *id > 0),
            photo_minithumbnail: v["photo"]["minithumbnail"]["data"].as_str()
                .filter(|value|value.len()<16_384&&value.chars().all(|c|c.is_ascii_alphanumeric()||matches!(c,'+'|'/'|'=')))
                .map(|value|format!("data:image/jpeg;base64,{value}")),
        })
    }

    fn ids(value: Value) -> Result<Vec<Value>> {
        Ok(value["chat_ids"]
            .as_array()
            .ok_or("Invalid chats response")?
            .clone())
    }

    fn load_list(
        g: &dyn TelegramGateway,
        list: &str,
        cancel: &AtomicBool,
        max_pages: usize,
    ) -> Result<()> {
        for _ in 0..max_pages {
            let result = g.request(
                json!({"@type":"loadChats","chat_list":{"@type":list},"limit":100}),
                cancel,
            );
            match result {
                Ok(_) => {}
                Err(e) if e.contains("error 404") => break, // TDLib: list fully loaded.
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn list(g: &dyn TelegramGateway, search: &str, scope: ChatListScope) -> Result<Vec<Chat>> {
        let cancel = AtomicBool::new(false);
        let query = search.trim();
        let query_lower = query.to_lowercase();
        // searchChats includes known archived chats too. Its ordering is Main-biased;
        // its IDs alone must NEVER determine membership in Main vs Archive.
        let matched_ids = if query.is_empty() {
            BTreeSet::new()
        } else {
            Self::ids(g.request(json!({"@type":"searchChats","query":query,"limit":100}), &cancel)?)?
                .into_iter()
                .filter_map(|id| id.as_i64().map(|v| v.to_string()))
                .collect::<BTreeSet<_>>()
        };
        let mut out = Vec::<Chat>::new();
        let mut seen = BTreeSet::<String>::new();
        for (list, is_archive) in [("chatListMain", false), ("chatListArchive", true)] {
            if scope == ChatListScope::Main && is_archive
                || scope == ChatListScope::Archive && !is_archive
            {
                continue;
            }
            // getChats(chatListX) is the authoritative list membership. In contrast,
            // searchChats isn't scoped, and getChat.positions may be temporarily stale.
            Self::load_list(g, list, &cancel, if query.is_empty() { 1 } else { 6 })?;
            let page = g.request(
                json!({"@type":"getChats","chat_list":{"@type":list},
                    "limit":if query.is_empty() { 100 } else { 600 }}),
                &cancel,
            )?;
            for id in Self::ids(page)? {
                let mut chat = Self::describe(g.request(
                    json!({"@type":"getChat","chat_id":id}), &cancel,
                )?)?;
                if !is_archive && chat.archived {
                    // A stale duplicate must not leak into the Main-only view.
                    continue;
                }
                chat.archived = is_archive;
                if !query.is_empty()
                    && !chat.title.to_lowercase().contains(&query_lower)
                    && !matched_ids.contains(&chat.id)
                {
                    continue;
                }
                if seen.insert(chat.id.clone()) {
                    out.push(chat);
                } else if is_archive
                    && let Some(old) = out.iter_mut().find(|item| item.id == chat.id)
                {
                    old.archived = true;
                }
            }
        }
        Ok(out)
    }
    /// Read only TDLib-owned media, sniff the actual file format, and strictly bound
    /// transfer size. The WebView receives no filesystem path.
    pub fn media_data(
        g: &dyn TelegramGateway, file_id: i64, files_root: &Path, full: bool
    ) -> Result<Option<String>> {
        if file_id <= 0 { return Ok(None); }
        let cancel = AtomicBool::new(false);
        let limit = if full { 24 * 1024 * 1024 } else { 2 * 1024 * 1024 };
        let file = g.request(json!({"@type":"downloadFile","file_id":file_id,
            "priority":if full {16} else {3},"offset":0,"limit":0,"synchronous":true}), &cancel)?;
        if file["local"]["is_downloading_completed"] != true { return Ok(None); }
        let Some(path) = file["local"]["path"].as_str() else { return Ok(None) };
        if path.is_empty() { return Ok(None); }
        // TDLib may return files in the Session file cache or database cache.
        // Both are account-owned. Canonicalize first to reject symlinks escaping the Session.
        let session = files_root.parent().ok_or("Invalid Session files directory")?;
        let canonical_session = std::fs::canonicalize(session).map_err(|_| "Session root unavailable")?;
        let path = Path::new(path);
        let resolved = if path.is_absolute() {
            std::fs::canonicalize(path)
        } else {
            std::fs::canonicalize(files_root.join(path))
                .or_else(|_| std::fs::canonicalize(session.join(path)))
        }.map_err(|_| "Media unavailable")?;
        if !resolved.starts_with(&canonical_session) || !resolved.is_file() {
            return Err("Media path outside current Session".into());
        }
        if std::fs::metadata(&resolved).map_err(|_| "Media metadata unavailable")?.len() > limit {
            return Ok(None);
        }
        let bytes = std::fs::read(resolved).map_err(|_| "Media read failed")?;
        if bytes.is_empty() || bytes.len() as u64 > limit { return Ok(None); }
        let mime = if bytes.starts_with(&[0xff,0xd8,0xff]) { "image/jpeg" }
            else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
            else if bytes.starts_with(b"RIFF") && bytes.get(8..12)==Some(b"WEBP") { "image/webp" }
            else if bytes.starts_with(b"GIF8") { "image/gif" }
            else if bytes.len()>12 && bytes.get(4..8)==Some(b"ftyp") { "video/mp4" }
            else { return Ok(None) };
        Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes))))
    }
    pub fn thumbnail(g: &dyn TelegramGateway, file_id: i64, files_root: &Path) -> Result<Option<String>> {
        Self::media_data(g, file_id, files_root, false)
    }
    pub fn public(g: &dyn TelegramGateway, username: &str) -> Result<Chat> {
        Self::describe(g.request(
            json!({"@type":"searchPublicChat","username":username.trim().trim_start_matches('@')}),
            &AtomicBool::new(false),
        )?)
    }
    pub fn saved(g: &dyn TelegramGateway) -> Result<Chat> {
        let c = AtomicBool::new(false);
        let me = g.request(json!({"@type":"getMe"}), &c)?;
        Self::describe(g.request(
            json!({"@type":"createPrivateChat","user_id":me["id"],"force":false}),
            &c,
        )?)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub source: MessageKey,
    pub state: String,
    pub detail: String,
    pub sent_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ForwardReport {
    pub items: Vec<ActionItem>,
}
pub struct ActionService;
impl ActionService {
    fn check_message(g: &dyn TelegramGateway, perms: &Value, source: i64, mid: i64) -> Result<()> {
        let c = AtomicBool::new(false);
        let p = g.request(
            json!({"@type":"getMessageProperties","chat_id":source,"message_id":mid}),
            &c,
        )?;
        if p["can_be_forwarded"] != true {
            return Err("Protected content or forwarding forbidden".into());
        }
        let m = g.request(
            json!({"@type":"getMessage","chat_id":source,"message_id":mid}),
            &c,
        )?;
        let field = match m["content"]["@type"].as_str().unwrap_or("") {
            "messageText" => "can_send_basic_messages",
            "messagePhoto" => "can_send_photos",
            "messageVideo" => "can_send_videos",
            "messageDocument" => "can_send_documents",
            "messageAudio" => "can_send_audios",
            "messageVoiceNote" => "can_send_voice_notes",
            "messageVideoNote" => "can_send_video_notes",
            "messagePoll" => "can_send_polls",
            _ => "can_send_other_messages",
        };
        if perms["all"] != true && perms[field] != true {
            return Err("Target does not permit this content type".into());
        }
        Ok(())
    }
    pub fn preview(
        g: &dyn TelegramGateway,
        target: &str,
        keys: &[MessageKey],
    ) -> Result<ForwardReport> {
        if keys.is_empty() || keys.len() > 5000 {
            return Err("Select 1–5000 messages".into());
        }
        let perms = Self::target_permissions(g, id(target)?)?;
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for k in keys {
            if !seen.insert(k.clone()) {
                continue;
            }
            let check = Self::check_message(g, &perms, id(&k.chat_id)?, id(&k.message_id)?);
            let (state, detail) = match check {
                Ok(()) => (
                    "Allowed",
                    "Permissions checked; will recheck on confirmation".into(),
                ),
                Err(e) => ("Rejected", e),
            };
            items.push(ActionItem {
                source: k.clone(),
                state: state.into(),
                detail,
                sent_id: None,
            });
        }
        Ok(ForwardReport { items })
    }
    pub fn link(g: &dyn TelegramGateway, k: &MessageKey, album: bool) -> Result<String> {
        let c = AtomicBool::new(false);
        let chat = id(&k.chat_id)?;
        let mid = id(&k.message_id)?;
        let p = g.request(
            json!({"@type":"getMessageProperties","chat_id":chat,"message_id":mid}),
            &c,
        )?;
        if p["can_get_link"] != true {
            return Err("Telegram does not allow a link for this message".into());
        }
        let v=g.request(json!({"@type":"getMessageLink","chat_id":chat,"message_id":mid,"media_timestamp":0,"for_album":album,"in_message_thread":false,"checklist_task_id":0,"poll_option_id":""}),&c)?;
        let link = v["link"].as_str().ok_or("Telegram returned no link")?;
        if !link.starts_with("https://t.me/") && !link.starts_with("tg://") {
            return Err("Unexpected Telegram link scheme".into());
        }
        Ok(link.into())
    }
    fn target_permissions(g: &dyn TelegramGateway, target: i64) -> Result<Value> {
        let c = AtomicBool::new(false);
        let chat = g.request(json!({"@type":"getChat","chat_id":target}), &c)?;
        if chat["type"]["@type"] == "chatTypePrivate" {
            let user = g.request(
                json!({"@type":"getUser","user_id":chat["type"]["user_id"]}),
                &c,
            )?;
            if user["type"]["@type"] == "userTypeDeleted" {
                return Err("Target user is deleted".into());
            }
            return Ok(json!({"all":true}));
        }
        let (method, key) = match chat["type"]["@type"].as_str() {
            Some("chatTypeSupergroup") => ("getSupergroup", "supergroup_id"),
            Some("chatTypeBasicGroup") => ("getBasicGroup", "basic_group_id"),
            _ => return Err("Unsupported target chat type".into()),
        };
        let mut body = json!({"@type":method});
        body[key] = chat["type"][key].clone();
        let group = g.request(body, &c)?;
        let status = &group["status"];
        let admin = status["@type"] == "chatMemberStatusAdministrator";
        let owner = status["@type"] == "chatMemberStatusCreator";
        if owner && status["is_member"] != false
            || admin
                && (group["is_channel"] != true || status["rights"]["can_post_messages"] == true)
        {
            return Ok(json!({"all":true}));
        }
        if group["is_channel"] == true {
            return Err("Target channel requires posting rights".into());
        }
        match status["@type"].as_str() {
            Some("chatMemberStatusMember") => Ok(chat["permissions"].clone()),
            Some("chatMemberStatusRestricted") if status["is_member"] == true => {
                let mut p = chat["permissions"].clone();
                if let Some(obj) = p.as_object_mut() {
                    for (k, v) in obj {
                        if k.starts_with("can_send_") {
                            *v = json!(
                                v == &Value::Bool(true)
                                    && status["permissions"][k.as_str()] == true
                            );
                        }
                    }
                }
                Ok(p)
            }
            _ => Err("No write membership in target chat".into()),
        }
    }
    pub fn forward(
        g: &dyn TelegramGateway,
        target: &str,
        keys: &[MessageKey],
    ) -> Result<ForwardReport> {
        if keys.is_empty() || keys.len() > 5000 {
            return Err("Select 1–5000 messages".into());
        }
        let target = id(target)?;
        let perms = Self::target_permissions(g, target)?;
        let c = AtomicBool::new(false);
        let mut report = ForwardReport { items: Vec::new() };
        let mut groups: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        for k in keys {
            groups
                .entry(id(&k.chat_id)?)
                .or_default()
                .insert(id(&k.message_id)?);
        }
        for (source, ids) in groups {
            let mut allowed = Vec::new();
            for mid in ids {
                let key = MessageKey {
                    chat_id: source.to_string(),
                    message_id: mid.to_string(),
                };
                let check = Self::check_message(g, &perms, source, mid);
                match check {
                    Ok(()) => allowed.push(mid),
                    Err(e) => report.items.push(ActionItem {
                        source: key,
                        state: "Rejected".into(),
                        detail: e,
                        sent_id: None,
                    }),
                }
            }
            for batch in allowed.chunks(100) {
                let r=g.request(json!({"@type":"forwardMessages","chat_id":target,"from_chat_id":source,"message_ids":batch,"topic_id":null,"message_thread_id":0,"options":{"@type":"messageSendOptions"},"send_copy":false,"remove_caption":false}),&c);
                for (i, mid) in batch.iter().enumerate() {
                    let (state, detail, sent_id) = match &r {
                        Err(e) => ("Failed", e.clone(), None),
                        Ok(v) => match v["messages"]
                            .as_array()
                            .and_then(|a| a.get(i))
                            .filter(|m| !m.is_null())
                        {
                            None => (
                                "Failed",
                                "Telegram returned null or omitted this message".into(),
                                None,
                            ),
                            Some(m) if m["id"].as_i64().is_none() => (
                                "Failed",
                                "Telegram returned a malformed message without an ID".into(),
                                None,
                            ),
                            Some(m)
                                if m["sending_state"]["@type"] == "messageSendingStateFailed" =>
                            {
                                ("Failed", "Telegram rejected sending".into(), None)
                            }
                            Some(m) => {
                                let pending = !m["sending_state"].is_null();
                                (
                                    if pending { "Pending" } else { "Sent" },
                                    if pending {
                                        "Accepted by TDLib; await send update".into()
                                    } else {
                                        "Sent".into()
                                    },
                                    m["id"].as_i64().map(|n| n.to_string()),
                                )
                            }
                        },
                    };
                    report.items.push(ActionItem {
                        source: MessageKey {
                            chat_id: source.to_string(),
                            message_id: mid.to_string(),
                        },
                        state: state.into(),
                        detail,
                        sent_id,
                    });
                }
            }
        }
        Ok(report)
    }
}
