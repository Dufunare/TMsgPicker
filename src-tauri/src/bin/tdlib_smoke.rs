use std::{
    ffi::{CStr, CString},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use tmsgpicker_lib::tdlib::{ffi, runtime::execute_sync};

fn send(client_id: i32, request: Value) {
    let request = CString::new(request.to_string()).unwrap();

    unsafe {
        ffi::td_send(client_id, request.as_ptr());
    }
}

fn receive(timeout: f64) -> Option<Value> {
    let ptr = unsafe { ffi::td_receive(timeout) };

    if ptr.is_null() {
        return None;
    }

    let owned = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };

    Some(serde_json::from_str(&owned).unwrap())
}

fn main() {
    // Phase 1: Synchronous FFI test
    let version = execute_sync(&json!({
        "@type": "getOption",
        "name": "version"
    }))
    .unwrap();

    println!("TDLib version: {version}");

    // Phase 2: Create a client
    let client_id = unsafe { ffi::td_create_client_id() };

    println!("TDLib client created: {client_id}");

    // Phase 3: Trigger initialization
    send(
        client_id,
        json!({
            "@type": "getAuthorizationState",
            "@extra": "bringup-auth"
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut saw_auth_state = false;

    while Instant::now() < deadline {
        let Some(update) = receive(1.0) else {
            continue;
        };

        if update["@client_id"].as_i64() != Some(client_id as i64) {
            continue;
        }

        println!("TDLib: {update}");

        if update["@type"] == "updateAuthorizationState"
            || update["@type"] == "authorizationStateWaitTdlibParameters"
        {
            saw_auth_state = true;
            break;
        }
    }

    assert!(saw_auth_state, "No authorization state received");

    // Phase 4: Graceful shutdown
    send(
        client_id,
        json!({
            "@type": "close"
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut closed = false;

    while Instant::now() < deadline {
        let Some(update) = receive(1.0) else {
            continue;
        };

        if update["@client_id"].as_i64() != Some(client_id as i64) {
            continue;
        }

        if update["@type"] == "updateAuthorizationState"
            && update["authorization_state"]["@type"] == "authorizationStateClosed"
        {
            closed = true;
            break;
        }
    }

    assert!(closed, "TDLib did not close cleanly");

    println!("PASS: TDLib client lifecycle");
}
