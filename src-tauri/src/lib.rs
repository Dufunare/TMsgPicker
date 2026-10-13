#[cfg(feature = "desktop")]
mod app;
pub mod core;
pub mod gateway;
pub mod services;
#[cfg(feature = "desktop")]
pub mod tdlib;
#[cfg(feature = "desktop")]
pub use app::run;
