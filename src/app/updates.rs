use std::{collections::HashMap, path::Path, process::Command, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::Serialize;

const RELEASES: &str = "https://github.com/autistic-computer/nidus/releases/";
const INSTALLER: &str = include_str!("../../install.sh");

#[derive(Serialize)]
struct Check {
    current: &'static str,
    latest: String,
    available: bool,
    can_install: bool,
    reason: Option<String>,
}

fn release_version(tag: &str) -> Result<Version> {
    let version = Version::parse(tag.strip_prefix('v').context("Invalid release tag")?)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "Automatic updates require a stable release"
    );
    Ok(version)
}

fn newer(current: &str, tag: &str) -> Result<bool> {
    Ok(release_version(tag)?
        .cmp_precedence(&Version::parse(current)?)
        .is_gt())
}

fn release_tag(url: &str) -> Result<String> {
    let tag = url
        .strip_prefix(RELEASES)
        .and_then(|path| path.strip_prefix("tag/"))
        .context("GitHub returned an unexpected release address")?;
    release_version(tag)?;
    Ok(tag.to_owned())
}

fn latest() -> Result<String> {
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--fail",
            "--silent",
            "--show-error",
            "--head",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--output",
            "/dev/null",
            "--write-out",
            "%{url_effective}",
        ])
        .arg(format!("{RELEASES}latest"))
        .output()
        .context("Install curl to check for Nidus updates")?;
    ensure!(
        output.status.success(),
        "Could not check for updates. Check your internet connection and try again. {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    release_tag(std::str::from_utf8(&output.stdout)?.trim())
}

fn owns(program: &str, arguments: &[&str], executable: &Path) -> Option<String> {
    let output = Command::new(program)
        .args(arguments)
        .arg(executable)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn install_reason(path: &Path, owner: Option<(&str, &str)>) -> Option<String> {
    match (path.to_str(), owner) {
        (Some("/usr/bin/nid"), Some(("dpkg", "nid"))) | (Some("/usr/local/bin/nid"), None) => None,
        (_, Some((manager, package))) => Some(format!(
            "Update {package} through {manager}, which manages this installation."
        )),
        _ => Some(
            "This copy was built locally. Install a Nidus release to enable updates here.".into(),
        ),
    }
}

fn installation() -> Result<Option<String>> {
    let path = std::env::current_exe()?;
    for (program, arguments, manager) in [
        ("rpm", &["-qf", "--qf", "%{NAME}"][..], "rpm"),
        ("pacman", &["-Qqo"][..], "pacman"),
    ] {
        if let Some(package) = owns(program, arguments, &path) {
            return Ok(install_reason(&path, Some((manager, &package))));
        }
    }
    let package = owns("dpkg-query", &["-S"], &path);
    let package = package
        .as_deref()
        .and_then(|line| line.split_once(": "))
        .and_then(|(name, _)| name.split(':').next());
    Ok(install_reason(&path, package.map(|name| ("dpkg", name))))
}

fn status() -> Result<Check> {
    let tag = latest()?;
    let reason = installation()?;
    Ok(Check {
        current: env!("CARGO_PKG_VERSION"),
        available: newer(env!("CARGO_PKG_VERSION"), &tag)?,
        latest: tag,
        can_install: reason.is_none(),
        reason,
    })
}

pub fn check() -> Result<()> {
    println!("{}", serde_json::to_string(&status()?)?);
    Ok(())
}

pub async fn remind(connection: zbus::Connection) {
    let start = tokio::time::Instant::now() + Duration::from_secs(60);
    let mut timer = tokio::time::interval_at(start, Duration::from_secs(6 * 60 * 60));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut announced = None;
    loop {
        timer.tick().await;
        if !automatic(read_settings().as_deref()) || metered().await {
            continue;
        }
        let check = match tokio::task::spawn_blocking(status).await {
            Ok(Ok(check)) => check,
            Ok(Err(error)) => {
                tracing::debug!(error = %format_args!("{error:#}"), "update check failed");
                continue;
            }
            Err(_) => continue,
        };
        if !check.available || !check.can_install || announced.as_ref() == Some(&check.latest) {
            continue;
        }
        match notify(&connection, &check.latest).await {
            Ok(()) => announced = Some(check.latest),
            Err(error) => tracing::warn!(%error, "could not show the update notification"),
        }
    }
}

fn read_settings() -> Option<Vec<u8>> {
    let config = super::gnome::xdg("XDG_CONFIG_HOME", ".config").ok()?;
    std::fs::read(config.join("nid/updates.json")).ok()
}

fn automatic(settings: Option<&[u8]>) -> bool {
    settings
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        .is_none_or(|settings| settings["automatic"] != false)
}

async fn metered() -> bool {
    async {
        let system = zbus::Connection::system().await?;
        let manager = zbus::Proxy::new(
            &system,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;
        Ok::<_, zbus::Error>(matches!(
            manager.get_property::<u32>("Metered").await?,
            1 | 3
        ))
    }
    .await
    .unwrap_or(false)
}

async fn notify(connection: &zbus::Connection, tag: &str) -> zbus::Result<()> {
    use zbus::{MatchRule, MessageStream, message::Type};
    const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
    let signal = |member| {
        Ok::<_, zbus::Error>(
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(NOTIFICATIONS)?
                .interface(NOTIFICATIONS)?
                .member(member)?
                .build(),
        )
    };
    let mut clicks =
        MessageStream::for_match_rule(signal("ActionInvoked")?, connection, None).await?;
    let mut closes =
        MessageStream::for_match_rule(signal("NotificationClosed")?, connection, None).await?;
    let notifications = zbus::Proxy::new(
        connection,
        NOTIFICATIONS,
        "/org/freedesktop/Notifications",
        NOTIFICATIONS,
    )
    .await?;
    let id: u32 = notifications.call("Notify", &notification(tag)).await?;
    tokio::spawn(async move {
        use super::desktop::next_message;
        loop {
            tokio::select! {
                Some(Ok(message)) = next_message(&mut clicks) => {
                    if message.body().deserialize::<(u32, String)>().is_ok_and(|(n, _)| n == id) {
                        open_settings();
                        return;
                    }
                }
                Some(Ok(message)) = next_message(&mut closes) => {
                    if message.body().deserialize::<(u32, u32)>().is_ok_and(|(n, _)| n == id) {
                        return;
                    }
                }
                else => return,
            }
        }
    });
    Ok(())
}

type Notification<'a> = (
    &'a str,
    u32,
    &'a str,
    String,
    &'a str,
    Vec<&'a str>,
    HashMap<&'a str, zbus::zvariant::Value<'a>>,
    i32,
);

fn notification(tag: &str) -> Notification<'static> {
    (
        "Nidus",
        0,
        "io.nid.nid",
        format!("Nidus {tag} is available"),
        "Open Nidus to install it.",
        vec!["default", "Open Nidus"],
        HashMap::from([("desktop-entry", "io.nid.nid".into())]),
        -1,
    )
}

fn open_settings() {
    tracing::info!("opening Nidus from the update notification");
    let opened = std::env::current_exe()
        .and_then(|nid| tokio::process::Command::new(nid).arg("settings").spawn());
    if let Err(error) = opened {
        tracing::warn!(%error, "could not open Nidus settings");
    }
}

pub fn install(tag: &str, gui: bool) -> Result<()> {
    ensure!(
        !nix::unistd::Uid::effective().is_root(),
        "Run updates as your desktop user, without sudo"
    );
    ensure!(
        newer(env!("CARGO_PKG_VERSION"), tag)?,
        "This release is not newer than the installed version"
    );
    if let Some(reason) = installation()? {
        bail!("{reason}");
    }
    let mut command = Command::new("bash");
    command.args([
        "-c",
        INSTALLER,
        "nid-update",
        "--update",
        env!("CARGO_PKG_VERSION"),
        "--no-launch",
        "--version",
        tag,
    ]);
    if gui {
        command.arg("--gui");
    }
    let status = command
        .status()
        .context("Could not start the Nidus installer")?;
    if status.code() == Some(126) {
        bail!("Update cancelled. Nothing was installed.");
    }
    ensure!(
        status.success(),
        "The update did not finish. See the installer message above and try again."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_follow_semver_and_only_use_stable_tags() {
        for (current, tag, expected) in [
            ("0.5.0", "v0.5.1", true),
            ("0.9.0", "v0.10.0", true),
            ("0.5.0", "v0.5.0", false),
            ("0.5.1", "v0.5.0", false),
            ("0.5.0-rc.1", "v0.5.0", true),
            ("0.6.0-rc.1", "v0.5.0", false),
            ("0.5.0+local", "v0.5.0", false),
        ] {
            assert_eq!(newer(current, tag).unwrap(), expected, "{current} -> {tag}");
        }
        for tag in [
            "0.5.1",
            "v0.5",
            "v01.5.1",
            "v0.5.1-rc.1",
            "v0.5.1+build",
            "v0.5.1/other",
            "v0.5.1?query",
            "v0.5.1\n",
        ] {
            assert!(newer("0.5.0", tag).is_err(), "{tag}");
        }
    }

    #[test]
    fn the_notification_matches_the_notify_signature() {
        use zbus::zvariant::Type;
        assert_eq!(
            Notification::SIGNATURE.to_string(),
            "(susssasa{sv}i)",
            "{:?}",
            notification("v0.6.0").3
        );
    }

    #[test]
    fn automatic_checks_stay_on_until_the_window_turns_them_off() {
        for (settings, expected) in [
            (None, true),
            (Some(&b"not json"[..]), true),
            (Some(br#"{}"#), true),
            (Some(br#"{"automatic":true}"#), true),
            (Some(br#"{"automatic":false}"#), false),
        ] {
            assert_eq!(automatic(settings), expected, "{settings:?}");
        }
    }

    #[test]
    fn only_the_projects_release_redirect_is_accepted() {
        assert_eq!(
            release_tag(&format!("{RELEASES}tag/v0.6.0")).unwrap(),
            "v0.6.0"
        );
        for url in [
            "http://github.com/autistic-computer/nidus/releases/tag/v0.6.0",
            "https://github.com/other/nid/releases/tag/v0.6.0",
            "https://github.com/autistic-computer/nidus/releases/latest",
            "https://github.com/autistic-computer/nidus/releases/tag/v0.6.0/download",
        ] {
            assert!(release_tag(url).is_err(), "{url}");
        }
    }

    #[test]
    fn updates_preserve_package_ownership() {
        assert!(install_reason(Path::new("/usr/local/bin/nid"), None).is_none());
        assert!(install_reason(Path::new("/usr/bin/nid"), Some(("dpkg", "nid"))).is_none());
        for (path, owner) in [
            ("/usr/bin/nid", None),
            ("/tmp/nid", None),
            ("/usr/bin/nid", Some(("dpkg", "another-package"))),
            ("/usr/bin/nid", Some(("pacman", "nid"))),
            ("/usr/local/bin/nid", Some(("rpm", "nid"))),
            ("/usr/local/bin/nid", Some(("dpkg", "nid"))),
        ] {
            assert!(install_reason(Path::new(path), owner).is_some(), "{path}");
        }
    }
}
