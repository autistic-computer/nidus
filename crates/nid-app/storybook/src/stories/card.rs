use ala::{AnyView, App, Context, Window};
use nid::{app::api::PeerState, core::KeyboardMode};
use nid_app::{model::Snapshot, pages, window::NidWindow};
use penna::WindowExt as _;
use penna_storybook::{Section, Story};

use crate::fixtures::{
    WINDOW, after_first_frame, app_window, beside, many_screens, nearby, one_peer, two_peers,
};

pub const STORY: Story = Story {
    id: "card",
    name: "Card",
    group: "Nidus",
    summary: "The card beside the desk for whatever is selected on it: a computer found nearby, this computer, or a paired one with its settings, crossings and screens.",
    sections: &[
        Section {
            id: "found-placeable",
            name: "Found: placeable",
            summary: "A computer found nearby, picked in the dock: one button per side and its mark.",
            build: found_placeable,
        },
        Section {
            id: "found-not-placeable",
            name: "Found: different version",
            summary: "It runs a different Nidus version, so the sides are disabled.",
            build: found_not_placeable,
        },
        Section {
            id: "this-computer",
            name: "This computer",
            summary: "This computer with two screens: the switch to move them one by one, a crossing to each neighbor, the note about its own screens and each screen's size.",
            build: this_computer,
        },
        Section {
            id: "this-computer-adjusting",
            name: "This computer: adjusting",
            summary: "Move screens one by one is on, so the desk outlines it dashed and offers Done.",
            build: this_computer_adjusting,
        },
        Section {
            id: "peer-connected",
            name: "Paired: connected",
            summary: "A connected computer: what it can do here, how its typing arrives, its crossings and screens, and Forget.",
            build: peer_connected,
        },
        Section {
            id: "peer-unreachable",
            name: "Paired: offline",
            summary: "A computer Nidus cannot reach.",
            build: peer_unreachable,
        },
        Section {
            id: "no-crossings",
            name: "No crossings",
            summary: "Placed apart from every other screen, so the pointer has nowhere to cross.",
            build: no_crossings,
        },
        Section {
            id: "keys-menu",
            name: "Keys menu",
            summary: "A computer set to PC key positions with reversed scrolling. Click its Keys button to open the menu of key layouts.",
            build: keys_menu,
        },
        Section {
            id: "forget-confirm",
            name: "Forget",
            summary: "The confirmation behind Forget.",
            build: forget_confirm,
        },
    ],
};

fn card(
    snapshot: Snapshot,
    selected: &'static str,
    window: &mut Window,
    cx: &mut App,
    then: impl FnOnce(&mut NidWindow, &mut Window, &mut Context<NidWindow>),
) -> AnyView {
    app_window(snapshot, WINDOW, window, cx, move |nid, window, cx| {
        nid.board
            .update(cx, |board, cx| board.select(Some(selected.into()), cx));
        then(nid, window, cx);
    })
}

fn docked() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.unplaced = nearby();
    snapshot
}

fn found_placeable(window: &mut Window, cx: &mut App) -> AnyView {
    card(docked(), "found:key:kitchen", window, cx, |_, _, _| {})
}

fn found_not_placeable(window: &mut Window, cx: &mut App) -> AnyView {
    card(docked(), "found:key:gaming", window, cx, |_, _, _| {})
}

fn this_computer(window: &mut Window, cx: &mut App) -> AnyView {
    card(many_screens(), "local", window, cx, |_, _, _| {})
}

fn this_computer_adjusting(window: &mut Window, cx: &mut App) -> AnyView {
    card(many_screens(), "local", window, cx, |nid, _, cx| {
        nid.board
            .update(cx, |board, cx| board.adjust("local", true, cx));
    })
}

fn peer_connected(window: &mut Window, cx: &mut App) -> AnyView {
    card(many_screens(), "peer:studio", window, cx, |_, _, _| {})
}

fn peer_unreachable(window: &mut Window, cx: &mut App) -> AnyView {
    card(two_peers(), "peer:tower", window, cx, |_, _, _| {})
}

fn no_crossings(window: &mut Window, cx: &mut App) -> AnyView {
    card(
        beside(PeerState::Connected, (2520, 400)),
        "peer:studio",
        window,
        cx,
        |_, _, _| {},
    )
}

fn keys_menu(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = one_peer(PeerState::Connected);
    for peer in &mut snapshot.peers {
        peer.keyboard = KeyboardMode::PcPositions;
        peer.reverse_scroll = true;
        peer.share_screen = true;
    }
    card(snapshot, "peer:studio", window, cx, |_, _, _| {})
}

fn forget_confirm(window: &mut Window, cx: &mut App) -> AnyView {
    card(
        one_peer(PeerState::Connected),
        "peer:studio",
        window,
        cx,
        |nid, window, _| {
            let model = nid.model.clone();
            after_first_frame(window, move |window, cx| {
                window.open_alert_dialog(cx, move |alert, _, _| {
                    pages::forget_alert(alert, "studio", &model)
                });
            });
        },
    )
}
