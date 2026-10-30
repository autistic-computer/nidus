mod clipboard;
mod desktop;
pub(crate) mod engine;
pub(crate) mod input;
pub(crate) mod ipc;
mod keys;
pub(crate) mod security;

pub use input::elevated;
pub use ipc::user_sid;

use std::path::PathBuf;
pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("Windows must provide LOCALAPPDATA"))
        .join("nid")
}
pub fn config_path() -> PathBuf {
    data_dir().join("nid.toml")
}
