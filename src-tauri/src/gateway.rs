use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub type Result<T> = std::result::Result<T, String>;

/// Requests execute on background workers, never the GUI thread.
pub trait TelegramGateway: Send + Sync {
    fn request(&self, request: Value, cancel: &AtomicBool) -> Result<Value>;
}

pub fn id(value: &str) -> Result<i64> {
    value.parse().map_err(|_| "Invalid Telegram ID".into())
}
