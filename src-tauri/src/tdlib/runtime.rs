use std::ffi::{CStr, CString};

use serde_json::Value;

use super::ffi;

pub fn execute_sync(request: &Value) -> Result<Value, String> {
    let request_json = request.to_string();

    let c_request = CString::new(request_json).map_err(|e| format!("Invalid request: {e}"))?;

    let response_ptr = unsafe { ffi::td_execute(c_request.as_ptr()) };

    if response_ptr.is_null() {
        return Err("td_execute returned NULL".into());
    }

    // Copy into Rust-owned memory immediately.
    let response = unsafe { CStr::from_ptr(response_ptr).to_string_lossy().into_owned() };

    serde_json::from_str(&response).map_err(|e| format!("Invalid TDLib response: {e}"))
}
