use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub type Result<T> = std::result::Result<T, String>;

/// Requests execute on background workers, never the GUI thread.
pub trait TelegramGateway: Send + Sync {
    fn request(&self, request: Value, cancel: &AtomicBool) -> Result<Value>;
    /// Submit every request in order before waiting; responses retain submission order.
    /// Wait for the last request first so bootstrap failures cannot strand queued requests.
    fn request_batch(&self, requests: Vec<Value>, cancel: &AtomicBool) -> Result<Vec<Value>>;
}

pub fn id(value: &str) -> Result<i64> {
    value.parse().map_err(|_| "Invalid Telegram ID".into())
}
