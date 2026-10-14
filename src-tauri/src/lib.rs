#[cfg(feature = "desktop")]
mod app;
pub mod core;
pub mod gateway;
pub mod services;
#[cfg(feature = "desktop")]
pub mod tdlib;
#[cfg(feature = "desktop")]
pub use app::run;
mod managed_data;
#[cfg(any(debug_assertions, test))]
pub mod reset;
pub mod settings;
