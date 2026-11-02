use std::time::Duration;

use ala::{AnyView, App, Window};
use nid::app::api::{Level, PeerState};
use nid_app::model::Snapshot;
use penna_storybook::{Section, Story};

use crate::fixtures::{
    WINDOW, after_first_frame, app_window, health, nearby, one_peer, settled, two_peers,
};

pub const STORY: Story = Story {
    id: "computers",
    name: "Computers",
    group: "Nidus",
    summary: "The main page once a computer is added: the desk with every computer's screens, problems above it and computers found nearby docked under it.",
    sections: &[
        Section {
            id: "loading",
            name: "Loading",
            summary: "Before the first snapshot arrives.",
            build: loading,
        },
        Section {
            id: "could-not-start",
            name: "Could not start",
            summary: "The window could not reach the background service and offers to try again.",
            build: could_not_start,
        },
        Section {
            id: "one-peer",
            name: "One computer",
            summary: "One computer placed to the right and connected.",
            build: |window, cx| page(one_peer(PeerState::Connected), window, cx),
        },
        Section {
            id: "two-peers",
            name: "Two computers",
            summary: "One connected to the right, one offline to the left; an offline computer's screens are dimmed.",
            build: |window, cx| page(two_peers(), window, cx),
        },
        Section {
            id: "controlling",
            name: "Controlling",
            summary: "The keyboard and pointer are over on the other computer.",
            build: |window, cx| page(one_peer(PeerState::ControlledFromHere), window, cx),
        },
        Section {
            id: "controlled",
            name: "Controlled",
            summary: "The other computer is using this one.",
            build: |window, cx| page(one_peer(PeerState::ControllingThis), window, cx),
        },
        Section {
            id: "paused",
            name: "Paused",
            summary: "Sharing is off, so the crossings are dimmed.",
            build: paused,
        },
        Section {
            id: "problems",
            name: "Problems",
            summary: "Health rows that are not fine show above the desk: an error and a warning with their fix buttons, and a warning without one.",
            build: problems,
        },
        Section {
            id: "nearby-docked",
            name: "Nearby",
            summary: "Computers found nearby wait in the dock under the desk; one runs a different version and cannot be placed.",
            build: nearby_docked,
        },
        Section {
            id: "failure-toast",
            name: "Failure toast",
            summary: "A request that failed while the window was open. The toast hides after a few seconds, so this section raises it again.",
            build: failure_toast,
        },
    ],
};

fn page(snapshot: Snapshot, window: &mut Window, cx: &mut App) -> AnyView {
    app_window(snapshot, WINDOW, window, cx, |_, _, _| {})
}

fn loading(window: &mut Window, cx: &mut App) -> AnyView {
    app_window(
        one_peer(PeerState::Connected),
        WINDOW,
        window,
        cx,
        |nid, _, cx| {
            nid.model.update(cx, |model, _| model.snapshot = None);
        },
    )
}

fn could_not_start(window: &mut Window, cx: &mut App) -> AnyView {
    app_window(
        one_peer(PeerState::Connected),
        WINDOW,
        window,
        cx,
        |nid, _, cx| {
            nid.model.update(cx, |model, _| {
                model.snapshot = None;
                model.error = Some(
                    "Nidus is not running: connecting to /run/user/1000/nid/control.sock failed: No such file or directory (os error 2)"
                        .into(),
                );
            });
        },
    )
}

fn paused(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.sharing = Some(false);
    page(settled(snapshot), window, cx)
}

fn problems(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.health = vec![
        health(
            "desktop",
            Level::Error,
            "Desktop helper",
            "The desktop helper is not running, so Nidus cannot move the pointer here. Install it again, then log out and back in.",
            Some(("Install…", "install_extension")),
        ),
        health(
            "computers",
            Level::Warning,
            "Paired computers",
            "studio refused the connection: its clock is 9 minutes off.",
            Some(("Retry", "retry")),
        ),
        health(
            "discovery",
            Level::Warning,
            "Nearby computers",
            "This network blocks discovery. Add computers by address instead.",
            None,
        ),
    ];
    page(settled(snapshot), window, cx)
}

fn nearby_docked(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.unplaced = nearby();
    page(snapshot, window, cx)
}

const FAILURE: &str = "Screens cannot overlap";

fn failure_toast(window: &mut Window, cx: &mut App) -> AnyView {
    app_window(
        one_peer(PeerState::Connected),
        WINDOW,
        window,
        cx,
        |nid, window, _| {
            let model = nid.model.downgrade();
            after_first_frame(window, move |_, cx| {
                cx.spawn(async move |cx| {
                    while model
                        .update(cx, |model, cx| {
                            model.failure = Some(FAILURE.into());
                            cx.notify();
                        })
                        .is_ok()
                    {
                        cx.background_executor()
                            .timer(Duration::from_millis(5400))
                            .await;
                    }
                })
                .detach();
            });
        },
    )
}
