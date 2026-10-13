use std::collections::HashSet;
use std::{
    env,
    error::Error,
    ffi::{CStr, CString},
    fs,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
use tmsgpicker_lib::tdlib::runtime::execute_sync;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tmsgpicker_lib::tdlib::ffi;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Only one thread may call td_receive.
// Copy the C string before the next receive/execute.
fn receive(timeout: f64) -> Result<Option<Value>> {
    let ptr = unsafe { ffi::td_receive(timeout) };

    if ptr.is_null() {
        return Ok(None);
    }

    let owned = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };

    Ok(Some(serde_json::from_str(&owned)?))
}

fn send(client_id: i32, request: Value, tag: &str) -> Result<()> {
    let mut request = request;
    request["@extra"] = json!(tag);

    let c_request = CString::new(request.to_string())?;

    unsafe {
        ffi::td_send(client_id, c_request.as_ptr());
    }

    Ok(())
}

fn input(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;

    Ok(line.trim().to_owned())
}

fn parameters() -> Result<Value> {
    let api_id: i32 = env::var("TMSGPICKER_API_ID")?.parse()?;
    let api_hash = env::var("TMSGPICKER_API_HASH")?;

    // Do not store account sessions in the Git repository.
    let root = PathBuf::from(env::var("LOCALAPPDATA")?)
        .join("com.funare.tmsgpicker")
        .join("tdlib");

    let db = root.join("db");
    let files = root.join("files");

    fs::create_dir_all(&db)?;
    fs::create_dir_all(&files)?;

    let passphrase = rpassword::prompt_password("TDLib database passphrase: ")?;

    if passphrase.is_empty() {
        return Err("Database passphrase must not be empty".into());
    }

    // TDLib bytes fields use Base64 in the JSON interface.
    let db_key = STANDARD.encode(passphrase.as_bytes());

    Ok(json!({
        "@type": "setTdlibParameters",
        "use_test_dc": false,

        "database_directory": db.to_string_lossy(),
        "files_directory": files.to_string_lossy(),
        "database_encryption_key": db_key,

        "use_file_database": true,
        "use_chat_info_database": true,
        "use_message_database": true,
        "use_secret_chats": false,

        "api_id": api_id,
        "api_hash": api_hash,

        "system_language_code": "zh-CN",
        "device_model": "TMsgPicker Desktop",
        "system_version": "Windows",
        "application_version": "0.1.0"
    }))
}

// Called when TDLib asks for an authorization action.
fn auth_request(state: &Value, params: &Value) -> Result<Value> {
    let kind = state["@type"].as_str().unwrap_or("");

    match kind {
        "authorizationStateWaitTdlibParameters" => {
            println!("Initializing TDLib...");
            Ok(params.clone())
        }

        "authorizationStateWaitPhoneNumber" => {
            let phone = input("Phone number (+countrycode...): ")?;

            Ok(json!({
                "@type": "setAuthenticationPhoneNumber",
                "phone_number": phone,
                "settings": null
            }))
        }

        "authorizationStateWaitCode" => {
            println!("Enter the code sent by Telegram.");
            let code = input("Login code: ")?;

            Ok(json!({
                "@type": "checkAuthenticationCode",
                "code": code
            }))
        }

        "authorizationStateWaitPassword" => {
            let password = rpassword::prompt_password("Telegram 2FA password: ")?;

            Ok(json!({
                "@type": "checkAuthenticationPassword",
                "password": password
            }))
        }

        "authorizationStateWaitEmailAddress" => {
            let email = input("Authentication email: ")?;

            Ok(json!({
                "@type": "setAuthenticationEmailAddress",
                "email_address": email
            }))
        }

        "authorizationStateWaitEmailCode" => {
            let code = input("Email verification code: ")?;

            Ok(json!({
                "@type": "checkAuthenticationEmailCode",
                "code": {
                    "@type": "emailAddressAuthenticationCode",
                    "code": code
                }
            }))
        }

        "authorizationStateWaitRegistration" => {
            Err("Registration required; not supported in this prototype".into())
        }

        "authorizationStateWaitPremiumPurchase" => {
            Err("Telegram requires an additional login step".into())
        }

        other => Err(format!("Unhandled authorization state: {other}").into()),
    }
}

fn authorize(client_id: i32, params: &Value) -> Result<()> {
    send(
        client_id,
        json!({"@type": "getAuthorizationState"}),
        "bootstrap",
    )?;

    let mut last_state: Option<Value> = None;
    let mut get_me_requested = false;

    loop {
        let Some(message) = receive(1.0)? else {
            continue;
        };

        if message["@client_id"].as_i64() != Some(i64::from(client_id)) {
            continue;
        }

        let kind = message["@type"].as_str().unwrap_or("");
        let tag = message["@extra"].as_str().unwrap_or("");

        // Authorization state is driven by updates.
        if kind == "updateAuthorizationState" {
            let state = &message["authorization_state"];
            let state_kind = state["@type"].as_str().unwrap_or("");

            println!("Authorization state: {state_kind}");

            match state_kind {
                "authorizationStateReady" => {
                    last_state = None;

                    if !get_me_requested {
                        get_me_requested = true;
                        send(client_id, json!({"@type": "getMe"}), "get-me")?;
                    }
                }

                "authorizationStateClosing" | "authorizationStateClosed" => {
                    return Err("Client closed during authorization".into());
                }

                _ => {
                    last_state = Some(state.clone());
                    let request = auth_request(state, params)?;
                    send(client_id, request, "auth")?;
                }
            }
            continue;
        }

        // TDLib rejected an authorization request.
        if tag == "auth" && kind == "error" {
            let code = message["code"].as_i64().unwrap_or(-1);
            let reason = message["message"].as_str().unwrap_or("Unknown TDLib error");

            eprintln!("Authorization error {code}: {reason}");

            if code == 429 {
                return Err("Telegram rate limit; stop retrying".into());
            }

            let state = last_state
                .as_ref()
                .ok_or("Authorization error without current state")?;

            if state["@type"] == "authorizationStateWaitTdlibParameters" {
                return Err(
                    "TDLib initialization failed; check API credentials and database key".into(),
                );
            }

            let request = auth_request(state, params)?;
            send(client_id, request, "auth")?;
            continue;
        }

        // We requested getMe after authorizationStateReady.
        if tag == "get-me" {
            if kind == "error" {
                return Err(format!("getMe failed: {}", message["message"]).into());
            }

            if kind == "user" {
                println!();
                println!("LOGIN SUCCESS");

                println!("User ID: {}", message["id"]);
                println!(
                    "Name: {} {}",
                    message["first_name"].as_str().unwrap_or(""),
                    message["last_name"].as_str().unwrap_or("")
                );

                return Ok(());
            }
        }
        if tag == "proxy" {
            match kind {
                "addedProxy" => {
                    println!("TDLib SOCKS5 proxy enabled.");
                }
                "error" => {
                    return Err(format!(
                        "TDLib proxy configuration failed: {}",
                        message["message"]
                    )
                    .into());
                }
                _ => {}
            }
            continue;
        }
    }
}

fn close_client(client_id: i32) -> Result<()> {
    send(client_id, json!({"@type": "close"}), "close")?;

    let deadline = Instant::now() + Duration::from_secs(20);

    while Instant::now() < deadline {
        let Some(message) = receive(1.0)? else {
            continue;
        };

        if message["@client_id"].as_i64() != Some(i64::from(client_id)) {
            continue;
        }

        if message["@type"] == "updateAuthorizationState"
            && message["authorization_state"]["@type"] == "authorizationStateClosed"
        {
            println!("TDLib closed cleanly.");
            return Ok(());
        }
    }

    Err("Timed out waiting for authorizationStateClosed".into())
}

fn request(client_id: i32, body: Value, tag: &str) -> Result<Value> {
    send(client_id, body, tag)?;

    let deadline = Instant::now() + Duration::from_secs(45);

    while Instant::now() < deadline {
        let Some(response) = receive(1.0)? else {
            continue;
        };

        if response["@client_id"].as_i64() != Some(i64::from(client_id)) {
            continue;
        }

        if response["@extra"].as_str() != Some(tag) {
            // Unsolicited TDLib updates are ignored only
            // in this temporary single-threaded smoke test.
            continue;
        }

        if response["@type"] == "error" {
            return Err(
                format!("TDLib error {}: {}", response["code"], response["message"]).into(),
            );
        }

        return Ok(response);
    }

    Err(format!("TDLib request timed out: {tag}").into())
}

fn fetch_history(client_id: i32, chat_id: i64, target_count: usize) -> Result<Vec<Value>> {
    let mut collected = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor: i64 = 0;

    // Safety guard for this bring-up test.
    for page in 0..200 {
        if collected.len() >= target_count {
            break;
        }

        let remaining = target_count - collected.len();
        let limit = remaining.min(100);

        let response = request(
            client_id,
            json!({
                "@type": "getChatHistory",
                "chat_id": chat_id,
                "from_message_id": cursor,
                "offset": 0,
                "limit": limit,
                "only_local": false
            }),
            &format!("history-page-{page}"),
        )?;

        let messages = response["messages"]
            .as_array()
            .ok_or("Invalid messages response")?;

        if messages.is_empty() {
            println!("Reached end of history.");
            break;
        }

        let mut next_cursor = None;
        let mut added = 0;

        for msg in messages {
            if msg.is_null() {
                continue;
            }

            let id = msg["id"].as_i64().ok_or("Invalid message ID")?;

            // Responses are ordered newest to oldest.
            next_cursor = Some(id);

            if seen.insert(id) {
                collected.push(msg.clone());
                added += 1;
            }
        }

        println!(
            "Page {}: returned={}, added={}, total={}",
            page + 1,
            messages.len(),
            added,
            collected.len()
        );

        let Some(next) = next_cursor else {
            println!("No usable message ID; stopping.");
            break;
        };

        // Stop rather than loop forever if the cursor
        // fails to move to an older message.
        if cursor != 0 && next >= cursor {
            println!(
                "Warning: history cursor did not advance \
                 (previous={cursor}, next={next})"
            );
            break;
        }

        cursor = next;
    }

    collected.truncate(target_count);
    Ok(collected)
}

fn test_history(client_id: i32) -> Result<()> {
    let username = env::var("TMSGPICKER_TEST_CHANNEL")?;
    let username = username.trim().trim_start_matches('@');

    // Step 1: Resolve public channel
    let chat = request(
        client_id,
        json!({
            "@type": "searchPublicChat",
            "username": username
        }),
        "resolve-chat",
    )?;

    if chat["@type"] != "chat" {
        return Err(format!("Unexpected chat response: {}", chat["@type"]).into());
    }

    let chat_id = chat["id"].as_i64().ok_or("Missing chat ID")?;

    println!("Channel: {}", chat["title"]);
    println!("Chat ID: {chat_id}");

    // Step 2: Fetch recent history
    let messages = fetch_history(client_id, chat_id, 50)?;

    println!("Messages returned: {}", messages.len());

    // Step 3: Inspect each message
    for msg in messages {
        if msg.is_null() {
            continue;
        }

        let metrics = &msg["interaction_info"];

        println!("\n----------------------------");
        println!("Message: {}", msg["id"]);
        println!("Date: {}", msg["date"]);
        println!("Content: {}", msg["content"]["@type"]);

        println!("Views: {}", metrics["view_count"]);
        println!("Forwards: {}", metrics["forward_count"]);
        println!("Comments: {}", metrics["reply_info"]["reply_count"]);

        if let Some(reactions) = metrics["reactions"]["reactions"].as_array() {
            for reaction in reactions {
                let kind = &reaction["type"];

                let label = match kind["@type"].as_str() {
                    Some("reactionTypeEmoji") => kind["emoji"].as_str().unwrap_or("?"),
                    Some("reactionTypeCustomEmoji") => "Custom Emoji",
                    Some("reactionTypePaid") => "Paid",
                    _ => "Unknown",
                };

                println!("Reaction {label}: {}", reaction["total_count"]);
            }
        }
        println!("Album ID: {}", msg["media_album_id"]);
    }

    println!("\nPASS: channel history fetched");
    Ok(())
}

fn main() -> Result<()> {
    let log_result = execute_sync(&json!({
        "@type": "setLogVerbosityLevel",
        "new_verbosity_level": 1
    }))?;

    if log_result["@type"] != "ok" {
        return Err(format!("Failed to configure TDLib logging: {log_result}").into());
    }
    let params = parameters()?;
    let client_id = unsafe { ffi::td_create_client_id() };

    println!("TDLib client ID: {client_id}");

    // Configure a local SOCKS5 proxy before authorization.
    send(
        client_id,
        json!({
            "@type": "addProxy",
            "proxy": {
                "@type": "proxy",
                "server": "127.0.0.1",
                "port": 7897,
                "type": {
                    "@type": "proxyTypeSocks5",
                    "username": "",
                    "password": ""
                }
            },
            "enable": true
        }),
        "proxy",
    )?;

    let test_result = authorize(client_id, &params).and_then(|_| test_history(client_id));

    let close_result = close_client(client_id);

    test_result?;
    close_result?;

    println!("PASS: TDLib history bring-up");

    println!("PASS: Telegram authorization and graceful shutdown");
    Ok(())
}
