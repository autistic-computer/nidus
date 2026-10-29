#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod geometry;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::{HOTKEY_LABEL, init, look, target};
