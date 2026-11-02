pub mod api;
#[cfg(feature = "runtime")]
pub mod client;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub(crate) mod desktop;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod desktop_agent;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod gnome;
#[cfg(feature = "runtime")]
pub(crate) mod handoff;
#[cfg(all(feature = "runtime", target_os = "macos"))]
mod host;
pub mod layout_model;
#[cfg(feature = "runtime")]
pub mod model;
#[cfg(all(feature = "runtime", target_os = "macos"))]
mod native;
#[cfg(all(feature = "runtime", target_os = "macos"))]
mod nearby;
#[cfg(all(feature = "runtime", target_os = "macos"))]
mod sharing;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod updates;
#[cfg(feature = "runtime")]
pub mod windows_view;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub(crate) use native::NativeApp;
