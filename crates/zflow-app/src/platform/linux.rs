use std::{collections::HashMap, path::PathBuf, process::Command, time::Duration};

use ala::{App, AppContext as _, Context, Entity, Task};
use anyhow::{Context as _, Result, anyhow, bail};
use serde::Deserialize;
use zbus::zvariant::OwnedValue;

use super::Updates;

const UUID: &str = "zflow@demfabris";
const SETUP_POLL: Duration = Duration::from_secs(2);
const UPDATE_INTERVAL: Duration = Duration::from_hours(6);

const ACTIVE: f64 = 1.0;
const INACTIVE: f64 = 2.0;
const ERROR: f64 = 3.0;
const OUT_OF_DATE: f64 = 4.0;
const INITIALIZED: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setup {
    Install,
    Enable,
    LogOut,
    Update,
    Error,
    Outdated,
    StoreOutdated,
}

impl Setup {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Install => "Install the zflow GNOME extension to finish setup",
            Self::Enable => "The zflow GNOME extension is turned off",
            Self::LogOut => "Log out and back in to finish setting up zflow",
            Self::Update => "Log out and back in to load the updated zflow GNOME extension",
            Self::Error => "The zflow GNOME extension stopped. Log out and back in to restart it",
            Self::Outdated => {
                "The zflow GNOME extension does not support this GNOME version. Update zflow"
            }
            Self::StoreOutdated => {
                "Update the zflow extension through GNOME Extensions or Extension Manager"
            }
        }
    }

    pub const fn button(self) -> Option<&'static str> {
        match self {
            Self::Install => Some("Install"),
            Self::Enable => Some("Turn on"),
            Self::LogOut | Self::Update | Self::Error => Some("Log out"),
            Self::Outdated | Self::StoreOutdated => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtensionFacts {
    pub state: Option<f64>,
    pub loaded_version: Option<String>,
    pub loaded_from_store: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub installed_from_store: bool,
}

pub fn setup_step(facts: &ExtensionFacts, older_extension: bool) -> Option<Setup> {
    let Some(state) = facts.state else {
        return Some(if facts.installed {
            Setup::LogOut
        } else {
            Setup::Install
        });
    };
    let from_store = facts.loaded_from_store || facts.installed_from_store;
    let pending = !facts.loaded_from_store
        && !facts.installed_from_store
        && facts
            .installed_version
            .as_ref()
            .is_some_and(|installed| facts.loaded_version.as_ref() != Some(installed));
    if from_store && (state == OUT_OF_DATE || older_extension) {
        Some(Setup::StoreOutdated)
    } else if state == OUT_OF_DATE {
        Some(Setup::Outdated)
    } else if pending {
        Some(Setup::Update)
    } else if state == INACTIVE || state == INITIALIZED {
        Some(Setup::Enable)
    } else if state == ERROR {
        Some(Setup::Error)
    } else {
        let _ = ACTIVE;
        None
    }
}

#[derive(Deserialize)]
struct Release {
    current: String,
    latest: String,
    available: bool,
    can_install: bool,
    reason: Option<String>,
}

#[derive(Deserialize, serde::Serialize)]
struct UpdatePreference {
    automatic: bool,
}

pub struct Services {
    pub updates: Updates,
    pub setup: Option<Setup>,
    pub setup_error: Option<String>,
    pub setup_busy: bool,
    pub installable: Option<String>,
    pub restart_needed: bool,
    pub installing: bool,
    older_extension: bool,
    _setup: Task<()>,
    _updates: Task<()>,
}

impl Services {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let setup = cx.spawn(async move |this, cx| {
            loop {
                let facts = cx
                    .background_executor()
                    .spawn(async { extension_facts() })
                    .await;
                let Ok(()) = this.update(cx, |services, cx| {
                    let step = facts
                        .ok()
                        .and_then(|facts| setup_step(&facts, services.older_extension));
                    if step != services.setup {
                        services.setup = step;
                        services.setup_error = None;
                        cx.notify();
                    }
                }) else {
                    return;
                };
                cx.background_executor().timer(SETUP_POLL).await;
            }
        });
        let updates = cx.spawn(async move |this, cx| {
            loop {
                let Ok(automatic) =
                    this.update(cx, |services, _| services.updates.automatic == Some(true))
                else {
                    return;
                };
                if automatic {
                    let _ = this.update(cx, |services, cx| services.check(true, cx));
                }
                cx.background_executor().timer(UPDATE_INTERVAL).await;
            }
        });
        Self {
            updates: Updates {
                automatic: Some(load_automatic()),
                ..Updates::default()
            },
            setup: None,
            setup_error: None,
            setup_busy: false,
            installable: None,
            restart_needed: false,
            installing: false,
            older_extension: false,
            _setup: setup,
            _updates: updates,
        }
    }

    pub fn observe(&mut self, snapshot: &crate::model::Snapshot) {
        self.older_extension = snapshot.health.iter().any(|row| {
            row.id == "desktop" && row.detail.contains("GNOME extension is older than the app")
        });
    }

    pub fn set_automatic(&mut self, automatic: bool, cx: &mut Context<Self>) {
        self.updates.automatic = Some(automatic);
        if let Err(error) = save_automatic(automatic) {
            self.updates.note = Some(format!("{error:#}"));
        } else if automatic {
            self.check(true, cx);
        }
        cx.notify();
    }

    pub fn check(&mut self, automatic: bool, cx: &mut Context<Self>) {
        if self.updates.checking || self.installing || self.restart_needed {
            return;
        }
        self.updates.checking = true;
        self.updates.note = Some("Checking for updates…".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if automatic && metered() {
                        return Ok(None);
                    }
                    run_update(&["check"]).and_then(|out| {
                        serde_json::from_str::<Release>(&out)
                            .map(Some)
                            .context("zflow update check gave an answer this window cannot read")
                    })
                })
                .await;
            let _ = this.update(cx, |services, cx| {
                services.updates.checking = false;
                services.installable = None;
                services.updates.available = None;
                services.updates.note = Some(match result {
                    Ok(None) => "Automatic checks are paused on a metered connection. You can still check manually.".into(),
                    Ok(Some(release)) => services.read(release),
                    Err(error) => format!("{error:#}"),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn read(&mut self, release: Release) -> String {
        if release.current != env!("CARGO_PKG_VERSION") {
            self.restart_needed = true;
            return format!("zflow {} is installed. Restart to use it.", release.current);
        }
        if release.available {
            self.updates.available = Some(release.latest.clone());
            if release.can_install {
                self.installable = Some(release.latest.clone());
                return format!("{} is available.", release.latest);
            }
            return format!(
                "{} is available. {}",
                release.latest,
                release.reason.unwrap_or_default()
            );
        }
        if release.can_install {
            "You have the latest version.".into()
        } else {
            release.reason.unwrap_or_default()
        }
    }

    pub fn install(&mut self, tag: String, cx: &mut Context<Self>) {
        if self.installing {
            return;
        }
        self.installing = true;
        self.updates.note =
            Some("Downloading and installing… Approve the system prompt to continue.".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let installed = tag.clone();
            let result = cx
                .background_executor()
                .spawn(async move { run_update(&["install", "--version", &installed, "--gui"]) })
                .await;
            let _ = this.update(cx, |services, cx| {
                services.installing = false;
                match result {
                    Ok(_) => {
                        services.restart_needed = true;
                        services.updates.note =
                            Some(format!("{tag} is installed. Restart to use it."));
                        restart(cx);
                    }
                    Err(error) => services.updates.note = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn act(&mut self, model: &Entity<crate::model::Model>, cx: &mut Context<Self>) {
        let Some(step) = self.setup else { return };
        self.setup_busy = true;
        self.setup_error = None;
        cx.notify();
        let install = (step == Setup::Install).then(|| {
            model
                .read(cx)
                .attempt(zflow::app::api::Request::InstallExtension)
        });
        cx.spawn(async move |this, cx| {
            let result = match install {
                Some(answer) => answer
                    .recv()
                    .await
                    .unwrap_or_else(|_| Err("zflow stopped".into()))
                    .map_err(|error| anyhow!(error)),
                None => {
                    cx.background_executor()
                        .spawn(async move { take_step(step) })
                        .await
                }
            };
            let _ = this.update(cx, |services, cx| {
                services.setup_busy = false;
                services.setup_error = result.err().map(|error| format!("{error:#}"));
                cx.notify();
            });
        })
        .detach();
    }
}

pub fn init(cx: &mut App) -> Entity<Services> {
    let services = cx.new(Services::new);
    cx.set_global(super::SharedServices(services.clone()));
    services
}

pub const LIVES_IN_TRAY: bool = false;

const BUS_NAME: &str = "io.zflow.zflow";
const BUS_PATH: &str = "/io/zflow/zflow";
const APPLICATION: &str = "org.freedesktop.Application";

enum Ask {
    Show(Option<String>),
    Quit,
}

const ACTIVATION_TOKEN: &str = "activation-token";

fn activation_token(mut platform_data: HashMap<String, OwnedValue>) -> Option<String> {
    platform_data
        .remove(ACTIVATION_TOKEN)
        .and_then(|value| String::try_from(value).ok())
}

static INSTANCE: std::sync::OnceLock<(zbus::blocking::Connection, async_channel::Receiver<Ask>)> =
    std::sync::OnceLock::new();

struct Instance(async_channel::Sender<Ask>);

#[zbus::interface(name = "org.freedesktop.Application")]
impl Instance {
    fn activate(&self, platform_data: HashMap<String, OwnedValue>) {
        let _ = self.0.try_send(Ask::Show(activation_token(platform_data)));
    }

    fn open(&self, _uris: Vec<String>, platform_data: HashMap<String, OwnedValue>) {
        let _ = self.0.try_send(Ask::Show(activation_token(platform_data)));
    }

    fn activate_action(
        &self,
        action: &str,
        _parameter: Vec<OwnedValue>,
        platform_data: HashMap<String, OwnedValue>,
    ) {
        let ask = if action == "quit" {
            Ask::Quit
        } else {
            Ask::Show(activation_token(platform_data))
        };
        let _ = self.0.try_send(ask);
    }
}

pub fn before_start(arguments: &crate::Arguments) -> bool {
    match claim_instance() {
        Ok(true) => !arguments.quit,
        Ok(false) => {
            if let Err(error) = ask_running(arguments.quit) {
                log::warn!("zflow could not reach the copy already running: {error:#}");
            }
            false
        }
        Err(error) => {
            log::warn!("zflow runs without checking for another copy: {error:#}");
            !arguments.quit
        }
    }
}

fn claim_instance() -> Result<bool> {
    zflow::app::client::on_bus_runtime(|| {
        let bus = zbus::blocking::Connection::session()?;
        let (asks, inbox) = async_channel::unbounded();
        bus.object_server().at(BUS_PATH, Instance(asks))?;
        match bus.request_name_with_flags(BUS_NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        {
            Ok(zbus::fdo::RequestNameReply::PrimaryOwner) => {
                let _ = INSTANCE.set((bus, inbox));
                Ok(true)
            }
            Ok(_) | Err(zbus::Error::NameTaken) => Ok(false),
            Err(error) => Err(error.into()),
        }
    })
}

fn ask_running(quit: bool) -> Result<()> {
    let bus = zflow::app::client::session_bus()?;
    let mut platform_data = HashMap::<&str, zbus::zvariant::Value>::new();
    let token = std::env::var("XDG_ACTIVATION_TOKEN").ok();
    if let Some(token) = &token {
        platform_data.insert(ACTIVATION_TOKEN, token.as_str().into());
    }
    if quit {
        bus.call_method(
            Some(BUS_NAME),
            BUS_PATH,
            Some(APPLICATION),
            "ActivateAction",
            &("quit", Vec::<zbus::zvariant::Value>::new(), platform_data),
        )?;
    } else {
        bus.call_method(
            Some(BUS_NAME),
            BUS_PATH,
            Some(APPLICATION),
            "Activate",
            &(platform_data,),
        )?;
    }
    Ok(())
}

fn answer_other_copies(cx: &mut App) {
    let Some((_, inbox)) = INSTANCE.get() else {
        return;
    };
    let inbox = inbox.clone();
    cx.spawn(async move |cx| {
        while let Ok(ask) = inbox.recv().await {
            let () = cx.update(|cx| match ask {
                Ask::Show(token) => {
                    crate::open(cx);
                    if let Some(token) = token
                        && let Some(window) = cx
                            .try_global::<crate::MainWindow>()
                            .and_then(|window| window.0)
                    {
                        let _ = window.update(cx, |_, window, _| {
                            window.activate_window_with_token(token);
                        });
                    }
                }
                Ask::Quit => cx.quit(),
            });
        }
    })
    .detach();
}

pub fn default_config() -> PathBuf {
    "/etc/zflow/zflow.toml".into()
}

pub fn start(
    model: &Entity<crate::model::Model>,
    services: &Entity<Services>,
    arguments: &crate::Arguments,
    cx: &mut App,
) {
    let services = services.clone();
    cx.observe(model, move |model, cx| {
        if let Some(snapshot) = &model.read(cx).snapshot {
            let snapshot = snapshot.clone();
            services.update(cx, |services, _| services.observe(&snapshot));
        }
    })
    .detach();
    answer_other_copies(cx);
    if !arguments.background {
        crate::open(cx);
    }
}

pub fn window_shown(_: &mut App) {}

pub fn may_close(cx: &App) -> bool {
    cx.try_global::<super::SharedServices>()
        .is_none_or(|services| !services.0.read(cx).installing)
}

pub fn notification_response(_: &ala::SystemNotificationResponse, _: &mut App) {}

pub fn restart(cx: &mut App) {
    if let Some((bus, _)) = INSTANCE.get() {
        let _ = zflow::app::client::on_bus_runtime(|| Ok(bus.release_name(BUS_NAME)?));
    }
    match Command::new(zflow_executable()).arg("settings").spawn() {
        Ok(_) => cx.quit(),
        Err(error) => log::warn!("zflow could not restart: {error}"),
    }
}

fn zflow_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("ZFLOW_EXECUTABLE") {
        return path.into();
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("zflow")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| "zflow".into())
}

fn run_update(arguments: &[&str]) -> Result<String> {
    let output = Command::new(zflow_executable())
        .arg("update")
        .args(arguments)
        .output()
        .context("Open zflow from Applications to check for updates.")?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = [stderr, stdout]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| "The update did not finish. Please try again.".into());
        bail!("{}", message.trim_start_matches("Error: "));
    }
    Ok(stdout)
}

fn preference_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("zflow/updates.json"))
}

fn load_automatic() -> bool {
    preference_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<UpdatePreference>(&bytes).ok())
        .is_none_or(|preference| preference.automatic)
}

fn save_automatic(automatic: bool) -> Result<()> {
    let path = preference_path().context("No home folder to keep the update setting in")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec(&UpdatePreference { automatic })?)?;
    Ok(())
}

fn metered() -> bool {
    let Ok(bus) = zflow::app::client::system_bus() else {
        return false;
    };
    let metered: Option<u32> = bus
        .call_method(
            Some("org.freedesktop.NetworkManager"),
            "/org/freedesktop/NetworkManager",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.NetworkManager", "Metered"),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
        .and_then(|value| u32::try_from(value).ok());
    matches!(metered, Some(1 | 3))
}

fn extension_facts() -> Result<ExtensionFacts> {
    let bus = zflow::app::client::session_bus()?;
    let info: HashMap<String, OwnedValue> = bus
        .call_method(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell",
            Some("org.gnome.Shell.Extensions"),
            "GetExtensionInfo",
            &UUID,
        )?
        .body()
        .deserialize()?;
    let metadata = installed_metadata();
    Ok(ExtensionFacts {
        state: info
            .get("state")
            .and_then(|value| f64::try_from(value).ok()),
        loaded_version: info
            .get("version-name")
            .and_then(|value| String::try_from(value.try_clone().ok()?).ok()),
        loaded_from_store: info.contains_key("_generated"),
        installed: metadata.is_some(),
        installed_version: metadata
            .as_ref()
            .and_then(|metadata| metadata.get("version-name")?.as_str().map(str::to_owned)),
        installed_from_store: metadata
            .as_ref()
            .is_some_and(|metadata| metadata.get("_generated").is_some()),
    })
}

fn installed_metadata() -> Option<serde_json::Value> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    data.into_iter()
        .chain(system.split(':').map(PathBuf::from))
        .find_map(|dir| {
            let bytes = std::fs::read(
                dir.join("gnome-shell/extensions")
                    .join(UUID)
                    .join("metadata.json"),
            )
            .ok()?;
            serde_json::from_slice(&bytes).ok()
        })
}

fn take_step(step: Setup) -> Result<()> {
    let bus = zflow::app::client::session_bus()?;
    match step {
        Setup::Enable => {
            let enabled: bool = bus
                .call_method(
                    Some("org.gnome.Shell"),
                    "/org/gnome/Shell",
                    Some("org.gnome.Shell.Extensions"),
                    "EnableExtension",
                    &UUID,
                )?
                .body()
                .deserialize()?;
            if !enabled {
                bail!("GNOME could not turn on the zflow extension");
            }
        }
        Setup::LogOut | Setup::Error | Setup::Update => {
            if step != Setup::Update {
                enable_at_login()?;
            }
            bus.call_method(
                Some("org.gnome.SessionManager"),
                "/org/gnome/SessionManager",
                Some("org.gnome.SessionManager"),
                "Logout",
                &0u32,
            )?;
        }
        Setup::Install | Setup::Outdated | Setup::StoreOutdated => {}
    }
    Ok(())
}

fn enable_at_login() -> Result<()> {
    let read = |key: &str| -> Result<Vec<String>> {
        let output = Command::new("gsettings")
            .args(["get", "org.gnome.shell", key])
            .output()
            .context("gsettings is missing")?;
        Ok(parse_strv(&String::from_utf8_lossy(&output.stdout)))
    };
    let write = |key: &str, list: &[String]| -> Result<()> {
        let value = format!(
            "[{}]",
            list.iter()
                .map(|uuid| format!("'{uuid}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Command::new("gsettings")
            .args(["set", "org.gnome.shell", key, &value])
            .status()?;
        Ok(())
    };
    let mut enabled = read("enabled-extensions")?;
    if !enabled.iter().any(|uuid| uuid == UUID) {
        enabled.push(UUID.into());
        write("enabled-extensions", &enabled)?;
    }
    let disabled = read("disabled-extensions")?;
    if disabled.iter().any(|uuid| uuid == UUID) {
        let kept: Vec<_> = disabled.into_iter().filter(|uuid| uuid != UUID).collect();
        write("disabled-extensions", &kept)?;
    }
    Ok(())
}

fn parse_strv(text: &str) -> Vec<String> {
    let text = text.trim().trim_start_matches("@as ");
    text.trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

use ala::{AnyElement, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use penna::{
    Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    settings::{SettingEntry, SettingsStack, settings_card_ground},
    switch::Switch,
};
use zflow::app::api::{Action, Health, Peer, Request};

use crate::{model::Snapshot, widgets, window::ZflowWindow};

pub const CAN_SHARE_SCREEN: bool = true;
pub const CAN_REFUSE_CONTROL: bool = true;

pub const OWN_SCREENS_NOTE: &str =
    "Between this computer’s own screens, the pointer follows GNOME’s display settings.";

pub fn banner(
    this: &mut ZflowWindow,
    _: &mut Window,
    cx: &mut Context<ZflowWindow>,
) -> Option<AnyElement> {
    let services = this.services.read(cx);
    let step = services.setup?;
    let title = services
        .setup_error
        .clone()
        .unwrap_or_else(|| step.title().to_owned());
    let busy = services.setup_busy;
    let action = step.button().map(|label| {
        Button::new("setup-step")
            .primary()
            .label(label)
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| {
                let model = this.model.clone();
                this.services
                    .update(cx, |services, cx| services.act(&model, cx));
            }))
            .into_any_element()
    });
    Some(
        div()
            .p(px(14.0))
            .pb_0()
            .child(widgets::banner(
                zflow::app::api::Level::Warning,
                title,
                "",
                action,
                cx,
            ))
            .into_any_element(),
    )
}

pub fn fix(_: &mut ZflowWindow, _: &str, _: &mut Window, _: &mut Context<ZflowWindow>) -> bool {
    false
}

pub fn health_action(row: &Health, _: &App) -> Option<Action> {
    row.action.clone()
}

pub fn first_run_cards(
    _: &mut ZflowWindow,
    _: &Snapshot,
    _: &mut Window,
    _: &mut Context<ZflowWindow>,
) -> Vec<AnyElement> {
    Vec::new()
}

pub fn display_settings_entry(cx: &App) -> Option<SettingEntry> {
    which("gnome-control-center")?;
    Some(
        SettingEntry::new("Display settings", "Arrange this computer’s own screens.").control(
            Button::new("display-settings")
                .label("Open")
                .on(settings_card_ground(cx))
                .on_click(|_, _, _| {
                    if let Err(error) = Command::new("gnome-control-center").arg("display").spawn()
                    {
                        log::warn!("could not open GNOME's display settings: {error}");
                    }
                }),
        ),
    )
}

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| PathBuf::from(dir).join(program))
        .find(|path| path.exists())
}

pub fn peer_entries(
    _: &mut ZflowWindow,
    _: &Peer,
    _: &mut Context<ZflowWindow>,
) -> Vec<SettingEntry> {
    Vec::new()
}

pub fn login_entry(
    _: &mut ZflowWindow,
    snapshot: &Snapshot,
    cx: &mut Context<ZflowWindow>,
) -> Option<SettingEntry> {
    let enabled = snapshot.autostart?;
    Some(
        SettingEntry::new(
            "Start at login",
            "Keep zflow available after you close this window.",
        )
        .control(
            Switch::new("autostart")
                .checked(enabled)
                .on_click(cx.listener(|this, on: &bool, _, cx| {
                    this.send(Request::SetAutostart { enabled: *on }, cx);
                })),
        ),
    )
}

pub fn settings_stacks(
    this: &mut ZflowWindow,
    _: &Snapshot,
    _: &mut Window,
    cx: &mut Context<ZflowWindow>,
) -> Vec<SettingsStack> {
    let services = this.services.read(cx);
    let updates = &services.updates;
    let installable = services.installable.clone();
    let restart_needed = services.restart_needed;
    let busy = updates.checking || services.installing;
    let (label, note) = if restart_needed {
        ("Restart", updates.note.clone())
    } else if installable.is_some() {
        ("Install…", updates.note.clone())
    } else {
        ("Check", updates.note.clone())
    };
    let act = Button::new("update-check")
        .label(label)
        .disabled(busy)
        .on(settings_card_ground(cx))
        .on_click(cx.listener(move |this, _, window, cx| {
            if restart_needed {
                restart(cx);
            } else if let Some(tag) = installable.clone() {
                confirm_install(this, tag, window, cx);
            } else {
                this.services
                    .update(cx, |services, cx| services.check(false, cx));
            }
        }));
    let automatic = updates.automatic.unwrap_or(true);
    vec![
        SettingsStack::titled("Updates")
            .child(
                SettingEntry::new(
                    format!("zflow {}", env!("CARGO_PKG_VERSION")),
                    note.unwrap_or_else(|| "Check for a new release.".into()),
                )
                .control(act),
            )
            .child(
                SettingEntry::new(
                    "Check automatically",
                    "Check when you open zflow and every six hours while it is open.",
                )
                .control(
                    Switch::new("update-automatic")
                        .checked(automatic)
                        .disabled(services.installing)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.services
                                .update(cx, |services, cx| services.set_automatic(*on, cx));
                        })),
                ),
            ),
    ]
}

fn confirm_install(
    this: &mut ZflowWindow,
    tag: String,
    window: &mut Window,
    cx: &mut Context<ZflowWindow>,
) {
    let services = this.services.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (services, tag) = (services.clone(), tag.clone());
        alert
            .title(format!("Install zflow {tag}?"))
            .description("Input sharing will disconnect briefly. zflow will ask for administrator approval, install the update, and restart this window. Your settings and paired computers will stay.")
            .button_props(
                DialogButtonProps::default()
                    .ok_text("Install and Restart")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                let tag = tag.clone();
                services.update(cx, |services, cx| services.install(tag, cx));
                true
            })
    });
}

pub fn this_computer_entries(
    _: &mut ZflowWindow,
    snapshot: &Snapshot,
    cx: &mut Context<ZflowWindow>,
) -> Vec<SettingEntry> {
    vec![
        SettingEntry::new(
            "Service log",
            "Follow what the background service is doing.",
        )
        .control(
            Button::new("open-logs")
                .label("Open")
                .on(settings_card_ground(cx))
                .on_click(cx.listener(|this, _, _, cx| this.send(Request::OpenLogs, cx))),
        ),
        SettingEntry::new("Configuration", snapshot.config_path.display().to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(state: Option<f64>) -> ExtensionFacts {
        ExtensionFacts {
            state,
            loaded_version: Some("0.7.0".into()),
            installed: true,
            installed_version: Some("0.7.0".into()),
            ..ExtensionFacts::default()
        }
    }

    #[test]
    fn the_setup_step_follows_gnome_and_the_disk() {
        let missing = ExtensionFacts::default();
        assert_eq!(setup_step(&missing, false), Some(Setup::Install));
        let on_disk = ExtensionFacts {
            installed: true,
            ..ExtensionFacts::default()
        };
        assert_eq!(setup_step(&on_disk, false), Some(Setup::LogOut));
        assert_eq!(setup_step(&facts(Some(ACTIVE)), false), None);
        assert_eq!(
            setup_step(&facts(Some(INACTIVE)), false),
            Some(Setup::Enable)
        );
        assert_eq!(
            setup_step(&facts(Some(INITIALIZED)), false),
            Some(Setup::Enable)
        );
        assert_eq!(setup_step(&facts(Some(ERROR)), false), Some(Setup::Error));
        assert_eq!(
            setup_step(&facts(Some(OUT_OF_DATE)), false),
            Some(Setup::Outdated)
        );
        let updated = ExtensionFacts {
            installed_version: Some("0.8.0".into()),
            ..facts(Some(ACTIVE))
        };
        assert_eq!(setup_step(&updated, false), Some(Setup::Update));
        let store = ExtensionFacts {
            loaded_from_store: true,
            ..facts(Some(ACTIVE))
        };
        assert_eq!(setup_step(&store, true), Some(Setup::StoreOutdated));
        assert_eq!(setup_step(&store, false), None);
    }

    #[test]
    fn gsettings_lists_read_back() {
        assert_eq!(parse_strv("@as []\n"), Vec::<String>::new());
        assert_eq!(
            parse_strv("['a@b', 'zflow@demfabris']\n"),
            vec!["a@b".to_owned(), "zflow@demfabris".to_owned()]
        );
    }
}
