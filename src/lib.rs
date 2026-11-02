#![cfg_attr(not(feature = "runtime"), allow(dead_code, unused_imports))]

pub mod app;
#[cfg(feature = "runtime")]
pub mod capture;
#[cfg(all(feature = "runtime", not(windows)))]
pub mod cli;
#[cfg(all(feature = "runtime", windows))]
#[path = "windows/cli.rs"]
pub mod cli;
pub mod clipboard;
pub mod config;
#[cfg(feature = "runtime")]
pub mod control;
pub mod core;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod daemon;
pub mod desktop;
#[cfg(feature = "runtime")]
pub mod discovery;
pub mod forgotten;
pub mod hello;
pub mod identity;
pub mod lens;
#[cfg(feature = "runtime")]
pub mod link;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod linux;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod macos;
#[cfg(feature = "runtime")]
pub mod metrics;
pub mod neighbors;
pub mod pairing_window;
#[cfg(feature = "runtime")]
pub mod peer_view;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod runtime;
#[cfg(feature = "runtime")]
pub mod session;
#[cfg(feature = "runtime")]
pub mod transport;
#[cfg(all(feature = "runtime", windows))]
pub mod windows;
pub mod wire;
