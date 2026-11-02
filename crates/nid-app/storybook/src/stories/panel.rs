use ala::{AnyView, App, Window};
use nid::app::api::{Level, PeerState};
use penna_storybook::{Section, Story};

use crate::fixtures::{self, alone, health, nearby, one_peer, settled};

pub const STORY: Story = Story {
    id: "panel",
    name: "Panel",
    group: "Nidus",
    summary: "The panel under the menu bar panel's icon: sharing, where the keyboard is, the computers and the way into the window. Each section is the panel at its real width and height.",
    sections: &[
        Section {
            id: "starting",
            name: "Starting",
            summary: "Before the first snapshot arrives.",
            build: |_, cx| fixtures::panel(None, cx),
        },
        Section {
            id: "ready",
            name: "Ready",
            summary: "One computer connected to the right.",
            build: |_, cx| fixtures::panel(Some(one_peer(PeerState::Connected)), cx),
        },
        Section {
            id: "away",
            name: "Away",
            summary: "The keyboard and pointer are on the other computer, with the keys that bring them back.",
            build: |_, cx| fixtures::panel(Some(one_peer(PeerState::ControlledFromHere)), cx),
        },
        Section {
            id: "visited",
            name: "Visited",
            summary: "The other computer is using this one, with a button to refuse it.",
            build: |_, cx| fixtures::panel(Some(one_peer(PeerState::ControllingThis)), cx),
        },
        Section {
            id: "problem",
            name: "Problem",
            summary: "A warning takes the status line and points to the window.",
            build: problem,
        },
        Section {
            id: "paused",
            name: "Paused",
            summary: "Sharing is off.",
            build: paused,
        },
        Section {
            id: "no-computers",
            name: "No computers",
            summary: "Nothing added yet.",
            build: |_, cx| fixtures::panel(Some(alone()), cx),
        },
        Section {
            id: "nearby",
            name: "Nearby",
            summary: "Computers found nearby that can be added, under the paired one; one on a different version is left out.",
            build: with_nearby,
        },
    ],
};

fn problem(_: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.health = vec![health(
        "computers",
        Level::Warning,
        "Paired computers",
        "studio refused the connection: its clock is 9 minutes off.",
        Some(("Retry", "retry")),
    )];
    fixtures::panel(Some(settled(snapshot)), cx)
}

fn paused(_: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.sharing = Some(false);
    fixtures::panel(Some(settled(snapshot)), cx)
}

fn with_nearby(_: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.unplaced = nearby();
    fixtures::panel(Some(snapshot), cx)
}
