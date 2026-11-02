use ala::{AnyView, App, Window};
use nid::app::api::{Level, PeerState};
use nid_app::model::{Page, Snapshot};
use penna_storybook::{Section, Story};

use crate::fixtures::{app_window, health, one_peer, settled, shortcuts};

const HEIGHT: f32 = 820.0;

pub const STORY: Story = Story {
    id: "settings",
    name: "Settings",
    group: "Nidus",
    summary: "The settings page behind the gear: general switches, shortcuts, checks, this computer and about.",
    sections: &[
        Section {
            id: "default",
            name: "Default",
            summary: "Start at login, clipboard sharing and pausing at edges, with no shortcuts or checks to show.",
            build: |window, cx| page(one_peer(PeerState::Connected), window, cx),
        },
        Section {
            id: "shortcuts",
            name: "Shortcuts",
            summary: "The key chords the background service listens for.",
            build: with_shortcuts,
        },
        Section {
            id: "checks",
            name: "Checks",
            summary: "Every health row: fine ones with a check, a warning and an error with their fix buttons.",
            build: checks,
        },
        Section {
            id: "toggles-hidden",
            name: "Switches hidden",
            summary: "A service that reports none of the general switches, so their rows are left out.",
            build: toggles_hidden,
        },
    ],
};

fn page(snapshot: Snapshot, window: &mut Window, cx: &mut App) -> AnyView {
    app_window(snapshot, HEIGHT, window, cx, |nid, _, cx| {
        nid.model
            .update(cx, |model, cx| model.show(Page::Settings, cx));
    })
}

fn with_shortcuts(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.shortcuts = shortcuts();
    page(snapshot, window, cx)
}

fn checks(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.health = vec![
        health("service", Level::Ok, "Background service", "Running", None),
        health(
            "computers",
            Level::Ok,
            "Paired computers",
            "No problems found.",
            None,
        ),
        health(
            "clipboard",
            Level::Warning,
            "Clipboard",
            "studio does not share its clipboard with this computer.",
            Some(("Retry", "retry")),
        ),
        health(
            "desktop",
            Level::Error,
            "Desktop helper",
            "The desktop helper is not running, so Nidus cannot move the pointer here.",
            Some(("Install…", "install_extension")),
        ),
    ];
    page(settled(snapshot), window, cx)
}

fn toggles_hidden(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.autostart = None;
    snapshot.share_clipboard = None;
    snapshot.pause_at_edges = None;
    page(snapshot, window, cx)
}
