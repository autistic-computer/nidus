#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(windows)]
pub use windows::*;

use serde::Deserialize;
use zz_gpui::Entity;

pub struct SharedServices(pub Entity<Services>);

impl zz_gpui::Global for SharedServices {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Platform {
    pub accessibility: Option<bool>,
    pub local_network: Option<LocalNetwork>,
    pub block_awdl: Option<bool>,
    pub elevated: Option<bool>,
    pub listen: Option<String>,
    pub addresses: Vec<String>,
    pub name: Option<String>,
    pub available: Option<bool>,
    pub notice: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetwork {
    #[default]
    Unknown,
    Allowed,
    Blocked,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Updates {
    pub unavailable: Option<String>,
    pub available: Option<String>,
    pub automatic: Option<bool>,
    pub install_automatically: Option<bool>,
    pub checking: bool,
    pub note: Option<String>,
}
