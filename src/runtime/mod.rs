#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod readiness;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "linux")]
pub use readiness::*;
