use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::layout_model::{Layout, Seam};
use crate::{
    config::PeerConfig, core::KeyboardMode, hello::Notice, neighbors::Unplaced,
    pairing_window::View as PairingWindow,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot<P> {
    pub status: Status,
    pub sharing: Option<bool>,
    pub health: Vec<Health>,
    pub pairing_window: PairingWindow,
    pub layout: Option<Layout>,
    #[serde(default)]
    pub seams: Vec<Seam>,
    pub own_mark: Option<String>,
    #[serde(default)]
    pub unplaced: Vec<Unplaced>,
    pub peers: Vec<Peer>,
    pub pause_at_edges: Option<bool>,
    #[serde(default)]
    pub shortcuts: Vec<Shortcut>,
    pub share_clipboard: Option<bool>,
    pub autostart: Option<bool>,
    pub config_path: PathBuf,
    #[serde(default)]
    pub notices: Vec<Notice>,
    pub platform: P,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state: State,
    pub peer: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Ready,
    Controlling,
    Controlled,
    Paused,
    Checking,
    Setup,
    Attention,
}

impl Status {
    pub fn new(sharing: Option<bool>, peers: &[Peer], health: &[Health], checking: bool) -> Self {
        let with = |state| peers.iter().find(|peer| peer.state == state);
        let (state, peer) = match sharing {
            None => (State::Attention, None),
            Some(false) => (State::Paused, None),
            Some(true) => {
                if let Some(peer) = with(PeerState::ControllingThis) {
                    (State::Controlled, Some(peer.name.clone()))
                } else if let Some(peer) = with(PeerState::ControlledFromHere) {
                    (State::Controlling, Some(peer.name.clone()))
                } else if peers.is_empty() {
                    (State::Setup, None)
                } else if health.iter().any(|row| row.level == Level::Error) {
                    (State::Attention, None)
                } else if checking {
                    (State::Checking, None)
                } else {
                    (State::Ready, None)
                }
            }
        };
        let title = match (state, &peer) {
            (State::Controlling, Some(peer)) => format!("Controlling {peer}"),
            (State::Controlled, Some(peer)) => format!("Controlled by {peer}"),
            (State::Ready, _) => "Ready".into(),
            (State::Paused, _) => "Paused".into(),
            (State::Checking, _) => "Checking…".into(),
            (State::Setup, _) => "Add a computer".into(),
            _ => "Needs attention".into(),
        };
        Self { state, peer, title }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub name: String,
    pub state: PeerState,
    pub detail: String,
    pub allow_control: bool,
    pub keyboard: KeyboardMode,
    pub reverse_scroll: bool,
    #[serde(default)]
    pub share_screen: bool,
    pub mark: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    Paired,
    Connecting,
    Connected,
    ControllingThis,
    ControlledFromHere,
    Unreachable,
}

impl Peer {
    pub fn new(name: &str, record: &PeerConfig, state: PeerState) -> Self {
        let detail = match state {
            PeerState::Paired => "Paired",
            PeerState::Connecting => "Connecting…",
            PeerState::Connected => "Connected",
            PeerState::ControllingThis => "Controlling this computer",
            PeerState::ControlledFromHere => "Controlled from here",
            PeerState::Unreachable => "Cannot reach this computer",
        };
        Self {
            name: name.to_owned(),
            state,
            detail: detail.into(),
            allow_control: record.permissions.send_normal,
            keyboard: record.keyboard,
            reverse_scroll: record.reverse_scroll,
            share_screen: record.share_screen,
            mark: record
                .spki_der()
                .map(|spki| crate::neighbors::mark(&spki))
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub label: String,
    pub command: String,
}

impl Health {
    pub fn new(id: &str, level: Level, title: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            level,
            title: title.into(),
            detail: detail.into(),
            action: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub title: String,
    pub keys: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Snapshot,
    SetSharing {
        enabled: bool,
    },
    SetPeer {
        name: String,
        allow_control: Option<bool>,
        keyboard: Option<KeyboardMode>,
        reverse_scroll: Option<bool>,
        #[serde(default)]
        share_screen: Option<bool>,
    },
    Forget {
        name: String,
    },
    MoveTile {
        id: String,
        x: i32,
        y: i32,
        tolerance: u32,
        #[serde(default)]
        computer: bool,
    },
    Preview {
        id: String,
        x: i32,
        y: i32,
        tolerance: u32,
        #[serde(default)]
        computer: bool,
    },
    Place {
        id: String,
        x: i32,
        y: i32,
        tolerance: u32,
    },
    AddAddress {
        address: String,
    },
    Retry,
    SetSwitching {
        pause_at_edges: bool,
    },
    SetClipboard {
        share: bool,
    },
    Lens {
        name: String,
    },
    SetAutostart {
        enabled: bool,
    },
    Reload,
    SetAwdl {
        enabled: bool,
    },
    HelperReady {
        ready: bool,
    },
    AllowAccessibility,
    CheckAccessibility,
    Discover,
    OpenSettings,
    OpenLogs,
    InstallExtension,
    TakeControl {
        name: String,
    },
    ReturnHere,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PeerPermissions;

    fn record(allow_control: bool) -> PeerConfig {
        PeerConfig {
            spki_der_hex: "01".into(),
            addresses: Vec::new(),
            permissions: PeerPermissions {
                connect: true,
                send_normal: allow_control,
                receive_normal: true,
                inject_prelogin: false,
            },
            keyboard: KeyboardMode::Mac,
            reverse_scroll: false,
            share_screen: false,
        }
    }

    #[test]
    fn status_puts_pauses_and_crossings_before_problems() {
        let broken = [Health::new("service", Level::Error, "Service", "down")];
        let desk = Peer::new("desk", &record(true), PeerState::Connected);
        let controlling = Peer::new("mac", &record(true), PeerState::ControllingThis);
        let controlled = Peer::new("mac", &record(true), PeerState::ControlledFromHere);
        let status = |sharing, peers: &[Peer], health: &[Health], checking| {
            let status = Status::new(sharing, peers, health, checking);
            (status.state, status.title)
        };
        assert_eq!(status(None, &[], &[], false).0, State::Attention);
        assert_eq!(
            status(
                Some(false),
                std::slice::from_ref(&controlling),
                &broken,
                false
            )
            .0,
            State::Paused
        );
        assert_eq!(
            status(Some(true), &[desk.clone(), controlling], &broken, false),
            (State::Controlled, "Controlled by mac".into())
        );
        assert_eq!(
            status(Some(true), &[controlled, desk.clone()], &[], false),
            (State::Controlling, "Controlling mac".into())
        );
        assert_eq!(status(Some(true), &[], &broken, false).0, State::Setup);
        let one = std::slice::from_ref(&desk);
        assert_eq!(status(Some(true), one, &broken, true).0, State::Attention);
        assert_eq!(status(Some(true), one, &[], true).0, State::Checking);
        let fine = [Health::new("service", Level::Ok, "Service", "Running")];
        assert_eq!(
            status(Some(true), one, &fine, false),
            (State::Ready, "Ready".into())
        );
    }

    #[test]
    fn peers_carry_their_permission_and_keyboard() {
        let peer = Peer::new("desk", &record(false), PeerState::ControlledFromHere);
        let value = serde_json::to_value(&peer).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "name": "desk", "state": "controlled_from_here", "detail": "Controlled from here",
                "allow_control": false, "keyboard": "mac", "reverse_scroll": false,
                "share_screen": false, "mark": crate::neighbors::mark(&[1]),
            })
        );
    }

    #[test]
    fn requests_use_the_shared_names_and_nothing_else() {
        let parse = |json| serde_json::from_str::<Request>(json);
        assert!(matches!(
            parse(r#"{"command":"move_tile","id":"local","x":1,"y":2,"tolerance":8}"#).unwrap(),
            Request::MoveTile { .. }
        ));
        assert!(matches!(
            parse(
                r#"{"command":"move_tile","id":"local","x":1,"y":2,"tolerance":8,"computer":true}"#
            )
            .unwrap(),
            Request::MoveTile { computer: true, .. }
        ));
        assert!(matches!(
            parse(r#"{"command":"preview","id":"local","x":1,"y":2,"tolerance":8}"#).unwrap(),
            Request::Preview {
                computer: false,
                ..
            }
        ));
        assert!(matches!(
            parse(r#"{"command":"set_clipboard","share":true}"#).unwrap(),
            Request::SetClipboard { share: true }
        ));
        assert!(matches!(
            parse(r#"{"command":"place","id":"key:ab","x":1,"y":2,"tolerance":8}"#).unwrap(),
            Request::Place { .. }
        ));
        assert!(matches!(
            parse(r#"{"command":"add_address","address":"100.64.0.7"}"#).unwrap(),
            Request::AddAddress { .. }
        ));
        for old in [
            r#"{"command":"pair_start","address":null}"#,
            r#"{"command":"pair","address":"192.0.2.7","code":"123456"}"#,
            r#"{"command":"pair_respond","allow":true}"#,
            r#"{"command":"pair_cancel"}"#,
            r#"{"command":"move","id":"local","x":1,"y":2,"tolerance":8}"#,
            r#"{"command":"set_keyboard","name":"desk","mode":"mac"}"#,
            r#"{"command":"set_peer","name":"desk","inject_prelogin":true}"#,
            r#"{"command":"set_sharing","enabled":true,"permissions":{"inject_prelogin":true}}"#,
            r#"{"command":"set_clipboard","share":"yes"}"#,
            r#"{"command":"set_clipboard","share":true,"files":true}"#,
            r#"{"command":"place","id":"key:ab","x":1,"y":2,"tolerance":8,"name":"desk"}"#,
            r#"{"command":"place","id":"key:ab","x":1,"y":2,"tolerance":8,"trust":true}"#,
            r#"{"command":"place","id":"key:ab","x":1,"y":2}"#,
            r#"{"command":"add_address","address":"100.64.0.7","port":43119}"#,
        ] {
            assert!(parse(old).is_err(), "{old}");
        }
    }

    #[test]
    fn the_snapshot_lists_the_shared_rows_in_window_order() {
        let snapshot = Snapshot {
            status: Status::new(Some(true), &[], &[], false),
            sharing: Some(true),
            health: Vec::new(),
            pairing_window: PairingWindow::default(),
            layout: None,
            seams: Vec::new(),
            own_mark: None,
            unplaced: Vec::new(),
            peers: Vec::new(),
            pause_at_edges: None,
            shortcuts: Vec::new(),
            share_clipboard: None,
            autostart: None,
            config_path: "/etc/nid/nid.toml".into(),
            notices: Vec::new(),
            platform: (),
        };
        let text = serde_json::to_string(&snapshot).unwrap();
        let at = |key| text.find(&format!("\"{key}\":")).unwrap();
        let order = [
            "status",
            "sharing",
            "health",
            "pairing_window",
            "layout",
            "seams",
            "own_mark",
            "unplaced",
            "peers",
            "pause_at_edges",
            "shortcuts",
            "share_clipboard",
            "autostart",
            "config_path",
            "notices",
            "platform",
        ];
        assert!(
            order.windows(2).all(|pair| at(pair[0]) < at(pair[1])),
            "{text}"
        );
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value.as_object().unwrap().len(), order.len());
        assert_eq!(value["status"]["title"], "Add a computer");
        assert_eq!(value["pairing_window"]["state"], "never");
        assert_eq!(value["unplaced"], serde_json::json!([]));
        assert_eq!(value["notices"], serde_json::json!([]));
    }
}
