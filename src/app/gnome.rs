use super::{
    api::{self, Action, Health, Level, Peer, PeerState, Request, Shortcut, Status},
    desktop::DesktopReceiver,
    layout_model::Layout,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const BUS: &str = "io.nid.Desktop";
pub(crate) const PATH: &str = "/io/nid/Desktop";
pub(crate) const API: u32 = 6;
const AUTOSTART: &str = "autostart/io.nid.desktop-agent.desktop";
const CONFIG_PATH: &str = "/etc/nid/nid.toml";

#[derive(Default)]
pub(super) struct State {
    pub receiver: DesktopReceiver,
}

pub(super) struct Service(pub Arc<Mutex<State>>);

#[zbus::interface(name = "io.nid.Desktop")]
impl Service {
    async fn call(&self, json: &str) -> zbus::fdo::Result<String> {
        self.dispatch(json)
            .await
            .map(|v| v.to_string())
            .map_err(|error| zbus::fdo::Error::Failed(format!("{error:#}")))
    }
}

impl Service {
    async fn dispatch(&self, json: &str) -> Result<serde_json::Value> {
        ensure!(json.len() <= 4096, "Desktop request exceeds limit");
        let request: Request = serde_json::from_str(json)?;
        use crate::peer_view::Request as DaemonRequest;
        match request {
            Request::Snapshot => {
                let daemon = crate::peer_view::status().await;
                let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
                let mut value = serde_json::to_value(snapshot(daemon, &state)?)?;
                value["api"] = API.into();
                return Ok(value);
            }
            Request::SetSharing { enabled } => {
                crate::peer_view::request(&DaemonRequest::SetSharing { enabled }).await?;
            }
            Request::Forget { name } => {
                crate::peer_view::request(&DaemonRequest::Forget { name }).await?;
            }
            Request::SetPeer {
                name,
                allow_control,
                keyboard,
                reverse_scroll,
                share_screen,
            } => {
                crate::peer_view::request(&DaemonRequest::SetPeer {
                    name,
                    allow_control,
                    keyboard,
                    reverse_scroll,
                    share_screen,
                })
                .await?;
            }
            Request::MoveTile {
                id,
                x,
                y,
                tolerance,
                computer,
            } => {
                crate::peer_view::request(&DaemonRequest::MoveTile {
                    id,
                    x,
                    y,
                    tolerance,
                    computer,
                })
                .await?;
            }
            Request::Preview {
                id,
                x,
                y,
                tolerance,
                computer,
            } => {
                let request = DaemonRequest::Preview {
                    id,
                    x,
                    y,
                    tolerance,
                    computer,
                };
                return match crate::peer_view::request(&request).await? {
                    crate::peer_view::DesktopReply::Preview { preview } => {
                        Ok(serde_json::to_value(preview)?)
                    }
                    _ => bail!("Unexpected response; update the Nidus service"),
                };
            }
            Request::SetAutostart { enabled } => set_autostart(enabled)?,
            Request::SetSwitching { pause_at_edges } => {
                crate::peer_view::request(&DaemonRequest::SetSwitching { pause_at_edges }).await?;
            }
            Request::SetClipboard { share } => {
                crate::peer_view::request(&DaemonRequest::SetClipboard { share }).await?;
            }
            Request::OpenSettings => {
                let mut child = tokio::process::Command::new(std::env::current_exe()?)
                    .arg("settings")
                    .spawn()?;
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
            Request::OpenLogs => {
                let mut child = log_viewers()
                    .iter()
                    .find_map(|(program, arguments)| {
                        tokio::process::Command::new(program)
                            .args(*arguments)
                            .spawn()
                            .ok()
                    })
                    .context("Install a terminal or GNOME Logs to read Nidus's log")?;
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
            Request::InstallExtension => {
                let connection = zbus::Connection::session().await?;
                let running = super::desktop::install_extension(Some(&connection)).await?;
                return Ok(serde_json::json!({"ok": true, "running": running}));
            }
            Request::Retry => {
                crate::peer_view::request(&DaemonRequest::Retry {}).await?;
            }
            Request::Place {
                id,
                x,
                y,
                tolerance,
            } => {
                crate::peer_view::request(&DaemonRequest::Place {
                    id,
                    x,
                    y,
                    tolerance,
                })
                .await?;
            }
            Request::AddAddress { address } => {
                let address = crate::peer_view::parse_address(&address)?;
                crate::peer_view::request(&DaemonRequest::AddAddress { address }).await?;
            }
            Request::Reload
            | Request::SetAwdl { .. }
            | Request::HelperReady { .. }
            | Request::Lens { .. }
            | Request::TakeControl { .. }
            | Request::ReturnHere
            | Request::AllowAccessibility
            | Request::CheckAccessibility
            | Request::Discover => bail!("Not available on this computer"),
        }
        Ok(serde_json::json!({"ok": true}))
    }
}

fn snapshot(
    daemon: Result<crate::peer_view::DesktopStatus>,
    state: &State,
) -> Result<api::Snapshot<()>> {
    let mut health = Vec::new();
    let (sharing, peers, shortcuts, layout) = match &daemon {
        Ok(daemon) => {
            let peers = peers(daemon);
            health.push(Health::new(
                "service",
                Level::Ok,
                "Background service",
                "Running",
            ));
            let ready = state.receiver.is_ready();
            health.push(Health::new(
                "desktop",
                if ready || !daemon.sharing {
                    Level::Ok
                } else {
                    Level::Error
                },
                "GNOME desktop",
                state.receiver.status(),
            ));
            if daemon.sharing {
                health.extend(link_health(&peers, &daemon.links));
            }
            (
                Some(daemon.sharing),
                peers,
                shortcuts(daemon),
                daemon.layout.clone(),
            )
        }
        Err(error) => {
            health.push(Health::new(
                "service",
                Level::Error,
                "Background service",
                format!("{error:#}"),
            ));
            (None, Vec::new(), Vec::new(), None)
        }
    };
    let connected = peers.iter().any(|peer| {
        matches!(
            peer.state,
            PeerState::Connected | PeerState::ControllingThis | PeerState::ControlledFromHere
        )
    });
    let checking = !connected && peers.iter().any(|peer| peer.state == PeerState::Connecting);
    let daemon = daemon.ok();
    Ok(api::Snapshot {
        status: Status::new(sharing, &peers, &health, checking),
        sharing,
        health,
        pairing_window: daemon
            .as_ref()
            .map(|daemon| daemon.pairing_window.clone())
            .unwrap_or_default(),
        seams: layout.as_ref().map(Layout::seams).unwrap_or_default(),
        layout,
        own_mark: daemon.as_ref().and_then(|daemon| daemon.own_mark.clone()),
        unplaced: daemon
            .as_ref()
            .map(|daemon| daemon.unplaced.clone())
            .unwrap_or_default(),
        peers,
        pause_at_edges: daemon.as_ref().map(|daemon| daemon.pause_at_edges),
        shortcuts,
        share_clipboard: daemon.as_ref().map(|daemon| daemon.share_clipboard),
        autostart: Some(autostart_enabled()?),
        config_path: CONFIG_PATH.into(),
        notices: daemon.map(|daemon| daemon.notices).unwrap_or_default(),
        platform: (),
    })
}

fn log_viewers() -> [(&'static str, &'static [&'static str]); 3] {
    const FOLLOW: &[&str] = &["journalctl", "--unit=nidd.service", "--follow"];
    [
        ("xdg-terminal-exec", FOLLOW),
        (
            "x-terminal-emulator",
            &["-e", "journalctl", "--unit=nidd.service", "--follow"],
        ),
        ("gnome-logs", &[]),
    ]
}

fn peers(daemon: &crate::peer_view::DesktopStatus) -> Vec<Peer> {
    use crate::peer_view::LinkStatus;
    let is = |peer: &Option<String>, name: &str| peer.as_deref() == Some(name);
    daemon
        .peers
        .iter()
        .map(|(name, record)| {
            let (state, reason) = if is(&daemon.receiving_from, name) {
                (PeerState::ControllingThis, None)
            } else if is(&daemon.sending_to, name) {
                (PeerState::ControlledFromHere, None)
            } else if daemon.connected.contains(name) {
                (PeerState::Connected, None)
            } else {
                match daemon.links.get(name) {
                    Some(LinkStatus::Connecting) => (PeerState::Connecting, None),
                    Some(LinkStatus::Unreachable { reason, .. }) => {
                        (PeerState::Unreachable, Some(reason))
                    }
                    None => (PeerState::Paired, None),
                }
            };
            let mut peer = Peer::new(name, record, state);
            if let Some(reason) = reason {
                peer.detail = reason.clone();
            }
            peer
        })
        .collect()
}

fn link_health(
    peers: &[Peer],
    links: &std::collections::BTreeMap<String, crate::peer_view::LinkStatus>,
) -> Option<Health> {
    use crate::peer_view::LinkStatus;
    if peers.is_empty() {
        return None;
    }
    let down: Vec<_> = peers
        .iter()
        .filter(|peer| peer.state == PeerState::Unreachable)
        .map(|peer| peer.name.as_str())
        .collect();
    let needs_fix = down.iter().any(|name| {
        matches!(
            links.get(*name),
            Some(LinkStatus::Unreachable {
                needs_fix: true,
                ..
            })
        )
    });
    let title = "Paired computers";
    Some(if !down.is_empty() {
        Health {
            action: Some(Action {
                label: "Retry".into(),
                command: "retry".into(),
            }),
            ..Health::new(
                "computers",
                if needs_fix {
                    Level::Error
                } else {
                    Level::Warning
                },
                title,
                format!("Cannot connect to {}", down.join(", ")),
            )
        }
    } else if peers.iter().any(|peer| peer.state == PeerState::Connecting) {
        Health::new("computers", Level::Ok, title, "Checking…")
    } else {
        Health::new("computers", Level::Ok, title, "No problems found.")
    })
}

fn shortcuts(daemon: &crate::peer_view::DesktopStatus) -> Vec<Shortcut> {
    [
        ("Switch to the other computer", &daemon.activation_chord),
        ("Return input to this computer", &daemon.escape_chord),
    ]
    .into_iter()
    .map(|(title, keys)| Shortcut {
        title: title.into(),
        keys: chord_label(keys),
    })
    .collect()
}

fn chord_label(keys: &[String]) -> String {
    keys.iter()
        .map(|key| {
            let key = key.trim_start_matches("KEY_");
            let key = key
                .strip_prefix("LEFT")
                .or_else(|| key.strip_prefix("RIGHT"))
                .unwrap_or(key);
            match key {
                "CTRL" => "Ctrl".into(),
                "META" => "Super".into(),
                "ALT" => "Alt".into(),
                "SHIFT" => "Shift".into(),
                key => {
                    let mut chars = key.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_string() + &chars.as_str().to_lowercase()
                    })
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

pub(super) async fn connect(state: Arc<Mutex<State>>) -> Result<zbus::Connection> {
    let connection = zbus::connection::Builder::session()?
        .serve_at(PATH, Service(state))?
        .build()
        .await?;
    connection
        .request_name_with_flags(BUS, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await
        .context("The Nidus desktop agent is already running")?;
    Ok(connection)
}

pub(super) fn xdg(variable: &str, fallback: &str) -> Result<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .context("HOME is not set")
}

pub(super) fn xdg_dirs(variable: &str, fallback: &str) -> Vec<PathBuf> {
    let value = std::env::var_os(variable).filter(|value| !value.is_empty());
    std::env::split_paths(value.as_deref().unwrap_or(fallback.as_ref()))
        .filter(|path| path.is_absolute())
        .collect()
}

pub(super) fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn entry_enabled(path: &Path) -> Result<bool> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(!text.lines().any(|line| {
            matches!(
                line.trim(),
                "Hidden=true" | "X-GNOME-Autostart-enabled=false"
            )
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

struct Autostart {
    user: PathBuf,
    system: Option<PathBuf>,
}

impl Autostart {
    fn find() -> Result<Self> {
        Ok(Self {
            user: xdg("XDG_CONFIG_HOME", ".config")?.join(AUTOSTART),
            system: xdg_dirs("XDG_CONFIG_DIRS", "/etc/xdg")
                .into_iter()
                .map(|dir| dir.join(AUTOSTART))
                .find(|path| path.is_file()),
        })
    }

    fn enabled(&self) -> Result<bool> {
        if self.user.symlink_metadata().is_ok() {
            return entry_enabled(&self.user);
        }
        self.system.as_deref().map_or(Ok(false), entry_enabled)
    }

    fn set(&self, enabled: bool) -> Result<()> {
        let system = match &self.system {
            Some(path) => entry_enabled(path)?,
            None => false,
        };
        if enabled == system {
            return remove_if_present(&self.user);
        }
        std::fs::create_dir_all(self.user.parent().unwrap())?;
        std::fs::write(
            &self.user,
            format!(
                "[Desktop Entry]\nType=Application\nName=Nidus\nExec=\"{}\" desktop-agent\nOnlyShowIn=GNOME;\nTerminal=false\n{}",
                executable()?,
                if enabled { "" } else { "Hidden=true\n" }
            ),
        )?;
        Ok(())
    }
}

fn autostart_enabled() -> Result<bool> {
    Autostart::find()?.enabled()
}

fn executable() -> Result<String> {
    let path = std::env::current_exe()?;
    let path = path.to_str().context("Executable path must be UTF-8")?;
    ensure!(
        !path.chars().any(char::is_control),
        "Invalid executable path"
    );
    Ok(path
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%"))
}

pub(super) fn set_autostart(enabled: bool) -> Result<()> {
    Autostart::find()?.set(enabled)
}

pub(super) fn install() -> Result<()> {
    let data = xdg("XDG_DATA_HOME", ".local/share")?;
    let system = xdg_dirs("XDG_DATA_DIRS", "/usr/local/share:/usr/share");
    for name in [
        "applications/io.nid.nid.desktop",
        "dbus-1/services/io.nid.Desktop.service",
    ] {
        if system.iter().any(|dir| dir.join(name).is_file()) {
            remove_if_present(&data.join(name))?;
        }
    }
    let autostart = Autostart::find()?;
    if autostart.user.symlink_metadata().is_err() || autostart.enabled()? {
        autostart.set(true)?;
    }
    Ok(())
}

pub(super) async fn start_agent(connection: &zbus::Connection, replace: bool) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let bus = zbus::fdo::DBusProxy::new(connection).await?;
    if let Ok(owner) = bus.get_name_owner(BUS.try_into()?).await {
        if !replace {
            return Ok(());
        }
        let pid = bus
            .get_connection_unix_process_id(owner.into_inner().into())
            .await?;
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if name.trim() != "nid" {
            return Ok(());
        }
        std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()?;
        for _ in 0..50 {
            if !bus.name_has_owner(BUS.try_into()?).await? {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    let _ = bus.reload_config().await;
    if let Err(error) = bus.start_service_by_name(BUS.try_into()?, 0).await {
        tracing::debug!(%error, "D-Bus could not start the desktop agent");
        std::process::Command::new(std::env::current_exe()?)
            .arg("desktop-agent")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .process_group(0)
            .spawn()?;
    }
    Ok(())
}

pub fn settings() -> Result<()> {
    use std::os::unix::process::CommandExt;
    ensure!(
        !nix::unistd::Uid::effective().is_root(),
        "Open settings as your desktop user, without sudo"
    );
    super::desktop::refresh_extension()?;
    if let Ok(cache) = xdg("XDG_CACHE_HOME", ".cache") {
        let _ = std::fs::remove_dir_all(cache.join("nid/desktop"));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async { start_agent(&zbus::Connection::session().await?, false).await })?;
    drop(runtime);
    let nid = std::env::current_exe()?;
    let mut app = nid.with_file_name("nid-app");
    if !app.is_file() {
        app = "nid-app".into();
    }
    Err(std::process::Command::new(&app)
        .env("NID_EXECUTABLE", &nid)
        .exec())
    .with_context(|| format!("Could not open {}. Reinstall Nidus", app.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a private bus: dbus-run-session -- cargo test app::gnome::tests -- --ignored"]
    async fn session_service_has_one_owner_and_rejects_extra_authority() {
        let connection = connect(Arc::new(Mutex::new(State::default())))
            .await
            .unwrap();
        assert!(
            connect(Arc::new(Mutex::new(State::default())))
                .await
                .is_err()
        );
        let client = zbus::Connection::session().await.unwrap();
        let proxy = zbus::Proxy::new(&client, BUS, PATH, BUS).await.unwrap();
        let result: zbus::Result<String> = proxy.call("Call", &(r#"{"command":"set_sharing","enabled":true,"permissions":{"inject_prelogin":true}}"#,)).await;
        assert!(result.is_err());
        let reply: String = proxy
            .call("Call", &(r#"{"command":"snapshot"}"#,))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&reply).unwrap()["api"],
            API
        );
        drop(connection);
    }

    #[test]
    fn the_service_state_becomes_peer_and_health_rows() {
        use crate::peer_view::LinkStatus;
        let mut config = crate::config::Config::default();
        for name in ["desk", "laptop", "mac", "new", "old"] {
            config.peers.insert(
                name.into(),
                crate::config::PeerConfig {
                    spki_der_hex: "01".into(),
                    addresses: Vec::new(),
                    permissions: crate::config::PeerPermissions {
                        connect: true,
                        send_normal: name != "old",
                        receive_normal: true,
                        inject_prelogin: false,
                    },
                    keyboard: crate::core::KeyboardMode::Standard,
                    reverse_scroll: false,
                    share_screen: false,
                },
            );
        }
        let reinstalled = "Reset or reinstalled. Drag its new tile onto its old one.";
        let status = crate::peer_view::DesktopStatus {
            receiving_from: Some("mac".into()),
            connected: vec!["desk".into(), "mac".into()],
            links: [
                (
                    "laptop".into(),
                    LinkStatus::Unreachable {
                        reason: reinstalled.into(),
                        needs_fix: true,
                    },
                ),
                ("new".into(), LinkStatus::Connecting),
                ("desk".into(), LinkStatus::Connecting),
            ]
            .into(),
            ..crate::peer_view::DesktopStatus::from_config(&config)
        };
        let rows: Vec<_> = peers(&status)
            .into_iter()
            .map(|peer| (peer.name, peer.state, peer.detail, peer.allow_control))
            .collect();
        let row = |name: &str, state, detail: &str, control| {
            (name.to_owned(), state, detail.to_owned(), control)
        };
        assert_eq!(
            rows,
            [
                row("desk", PeerState::Connected, "Connected", true),
                row("laptop", PeerState::Unreachable, reinstalled, true),
                row(
                    "mac",
                    PeerState::ControllingThis,
                    "Controlling this computer",
                    true
                ),
                row("new", PeerState::Connecting, "Connecting…", true),
                row("old", PeerState::Paired, "Paired", false),
            ]
        );
        let health = link_health(&peers(&status), &status.links).unwrap();
        assert_eq!(
            (health.level, health.detail.as_str()),
            (Level::Error, "Cannot connect to laptop")
        );
        assert_eq!(health.action.unwrap().command, "retry");
        let mut asleep = status.links.clone();
        asleep.insert(
            "laptop".into(),
            LinkStatus::Unreachable {
                reason: "input connection to 192.0.2.7:43119 timed out".into(),
                needs_fix: false,
            },
        );
        let health = link_health(&peers(&status), &asleep).unwrap();
        assert_eq!(health.level, Level::Warning);
        assert!(health.action.is_some());
        assert_eq!(
            shortcuts(&status)[1].keys,
            "Ctrl+Super+Backspace",
            "the default escape chord"
        );
        let layout = crate::app::layout_model::Layout {
            monitors: vec![crate::app::layout_model::Monitor {
                display: None,
                id: "local".into(),
                label: "This computer".into(),
                peer: None,
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            }],
        };
        let fedora = crate::neighbors::Unplaced {
            id: "key:aa".into(),
            name: "fedora".into(),
            os: Some(crate::wire::Os::Linux),
            mark: Some("1abc9e".into()),
            version: Some("0.3.0".into()),
            state: crate::neighbors::UnplacedState::Ready,
            via: crate::neighbors::Via::Mdns,
        };
        let joined = crate::hello::Notice {
            id: 1,
            kind: crate::hello::NoticeKind::Joined,
            name: "desk".into(),
            mark: "a1b2c3".into(),
        };
        let up = snapshot(
            Ok(crate::peer_view::DesktopStatus {
                layout: Some(layout.clone()),
                share_clipboard: true,
                own_mark: Some("a1b2c3".into()),
                unplaced: vec![fedora.clone()],
                notices: vec![joined.clone()],
                ..status
            }),
            &State::default(),
        )
        .unwrap();
        assert_eq!(up.layout, Some(layout));
        assert_eq!(up.share_clipboard, Some(true));
        assert_eq!(up.own_mark.as_deref(), Some("a1b2c3"));
        assert_eq!((up.unplaced, up.notices), (vec![fedora], vec![joined]));
        assert!(
            up.health
                .iter()
                .any(|row| row.id == "computers" && row.level == Level::Error)
        );

        let down = snapshot(
            Err(anyhow::anyhow!("Start the Nidus system service")),
            &State::default(),
        )
        .unwrap();
        assert_eq!(down.sharing, None);
        assert_eq!(down.status.state, api::State::Attention);
        assert_eq!(down.health[0].level, Level::Error);
        assert_eq!(down.health[0].detail, "Start the Nidus system service");
        assert!(down.peers.is_empty() && down.shortcuts.is_empty() && down.layout.is_none());
        assert_eq!(down.share_clipboard, None);
    }

    #[test]
    fn chords_read_like_keys() {
        let keys = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            chord_label(&keys(&["KEY_LEFTCTRL", "KEY_LEFTMETA", "KEY_F12"])),
            "Ctrl+Super+F12"
        );
        assert_eq!(
            chord_label(&keys(&["KEY_RIGHTALT", "KEY_LEFTSHIFT", "KEY_BACKSPACE"])),
            "Alt+Shift+Backspace"
        );
    }

    #[test]
    fn start_at_login_overrides_the_system_entry_instead_of_deleting_it() {
        let temp = tempfile::tempdir().unwrap();
        let system = temp.path().join("etc/autostart.desktop");
        std::fs::create_dir_all(system.parent().unwrap()).unwrap();
        std::fs::write(&system, "[Desktop Entry]\nExec=nid desktop-agent\n").unwrap();
        let autostart = Autostart {
            user: temp
                .path()
                .join("home/autostart/io.nid.desktop-agent.desktop"),
            system: Some(system),
        };
        assert!(autostart.enabled().unwrap(), "the package turns it on");
        autostart.set(false).unwrap();
        assert!(!autostart.enabled().unwrap());
        let text = std::fs::read_to_string(&autostart.user).unwrap();
        assert!(text.contains("\nHidden=true\n"), "{text}");
        autostart.set(true).unwrap();
        assert!(autostart.enabled().unwrap());
        assert!(!autostart.user.exists(), "the system entry applies again");

        let autostart = Autostart {
            system: None,
            ..autostart
        };
        assert!(!autostart.enabled().unwrap());
        autostart.set(true).unwrap();
        assert!(autostart.enabled().unwrap());
        assert!(
            !std::fs::read_to_string(&autostart.user)
                .unwrap()
                .contains("Hidden")
        );
        autostart.set(false).unwrap();
        assert!(!autostart.user.exists());
    }
}
