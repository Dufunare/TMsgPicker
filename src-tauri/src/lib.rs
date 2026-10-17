#[cfg(feature = "desktop")]
mod app;
pub mod core;
pub mod formatted_text;
// Available to desktop runtime; also included in no-default-features unit
// tests without creating unused-code warnings in the production core library.
#[cfg(any(feature = "desktop", test))]
mod login_code_diagnostics;
pub mod scan;
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
pub mod settings_target;
