pub mod api;
pub mod client;
#[cfg(target_os = "linux")]
pub(crate) mod desktop;
#[cfg(target_os = "linux")]
pub mod desktop_agent;
#[cfg(target_os = "linux")]
pub mod gnome;
pub(crate) mod handoff;
#[cfg(target_os = "macos")]
mod host;
pub mod layout_model;
pub mod model;
#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
mod nearby;
#[cfg(target_os = "macos")]
mod sharing;
#[cfg(target_os = "linux")]
pub mod updates;
pub mod windows_view;
#[cfg(target_os = "macos")]
pub(crate) use native::NativeApp;
