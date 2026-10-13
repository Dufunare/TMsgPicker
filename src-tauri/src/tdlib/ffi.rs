use std::ffi::{c_char, c_double, c_int};

#[link(name = "tdjson", kind = "dylib")]
unsafe extern "C" {
    pub fn td_create_client_id() -> c_int;

    pub fn td_send(client_id: c_int, request: *const c_char);

    pub fn td_receive(timeout: c_double) -> *const c_char;

    pub fn td_execute(request: *const c_char) -> *const c_char;
}
