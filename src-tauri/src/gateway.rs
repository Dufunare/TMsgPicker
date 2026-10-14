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

pub trait SessionLifecycle: TelegramGateway {
    fn begin_close(&self);
    fn wait_closed(&self) -> Result<()>;
    /// A timed out request can still be executing. Never resubmit or append close.
    fn logout_in_progress(&self) -> bool {
        false
    }
    /// Positive receipt or LoggingOut observed on this exact client.
    fn logout_acknowledged(&self) -> bool {
        false
    }
    fn wait_logout_closed(&self) -> Result<()> {
        self.wait_closed()
    }
}

pub fn id(value: &str) -> Result<i64> {
    value.parse().map_err(|_| "Invalid Telegram ID".into())
}
