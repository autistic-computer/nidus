use ala::{AnyView, App, Window};
use nid::{
    app::api::PeerState,
    neighbors::{UnplacedState, Via},
    pairing_window::{Holding, Reason, State, View},
    wire::Os,
};
use nid_app::pages::first_run::Step;
use penna_storybook::{Section, Story};

use crate::fixtures::{
    self, GAMING, KITCHEN, STUDIO, TOWER, WINDOW, after_first_frame, alone, app_window, found,
    one_peer,
};

pub const STORY: Story = Story {
    id: "first-run",
    name: "First run",
    group: "Nidus",
    summary: "The window before any computer is added: pick one found nearby, place it beside your screens, then try crossing over and back. Each section is the real window on a fixture snapshot.",
    sections: &[
        Section {
            id: "find-empty",
            name: "Find: nothing yet",
            summary: "No computer found on this network yet, with the install command to run on the other one.",
            build: find_empty,
        },
        Section {
            id: "find-found",
            name: "Find: computers nearby",
            summary: "One found in each state: ready, identifying, a different Nidus version and a duplicate name; found nearby and added by address, with and without a mark. The pairing window is open, so the note says how long this computer accepts one by itself.",
            build: find_found,
        },
        Section {
            id: "find-holding",
            name: "Find: accepting by itself",
            summary: "The only new computer around is being accepted while the pairing window is open.",
            build: find_holding,
        },
        Section {
            id: "find-copied",
            name: "Find: command copied",
            summary: "Copy pressed. Two new computers showed up, so this computer waits for you to choose.",
            build: find_copied,
        },
        Section {
            id: "place",
            name: "Place: adding",
            summary: "Right after This one: the computer is not added yet, so Done waits.",
            build: place,
        },
        Section {
            id: "place-placed",
            name: "Place: placed",
            summary: "Added, connected and placed to the right.",
            build: place_placed,
        },
        Section {
            id: "place-not-answering",
            name: "Place: not answering",
            summary: "Added but not connected yet, with the hint to add this computer from the other one.",
            build: place_not_answering,
        },
        Section {
            id: "try-waiting",
            name: "Try: waiting",
            summary: "The new computer is still connecting.",
            build: try_waiting,
        },
        Section {
            id: "try-reached",
            name: "Try: reached",
            summary: "The pointer went over; coming back is the last step.",
            build: try_reached,
        },
        Section {
            id: "try-back",
            name: "Try: back",
            summary: "Over and back again: every step checked and Finish is the primary button.",
            build: try_back,
        },
        Section {
            id: "added-by",
            name: "Added by another computer",
            summary: "Another computer added this one, so the window shows its mark to compare and offers to forget it.",
            build: added_by,
        },
        Section {
            id: "add-address",
            name: "Add by address",
            summary: "The dialog behind By address, for a computer Nidus cannot find on this network.",
            build: add_address,
        },
        Section {
            id: "add-address-error",
            name: "Add by address: refused",
            summary: "Nidus refused the address and says why under the field.",
            build: add_address_error,
        },
        Section {
            id: "add-address-looking",
            name: "Add by address: looking up",
            summary: "Look up pressed; the dialog waits for Nidus to answer.",
            build: add_address_looking,
        },
    ],
};

fn find(snapshot: nid_app::model::Snapshot, window: &mut Window, cx: &mut App) -> AnyView {
    app_window(snapshot, WINDOW, window, cx, |_, _, _| {})
}

fn find_empty(window: &mut Window, cx: &mut App) -> AnyView {
    find(alone(), window, cx)
}

fn find_found(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = alone();
    snapshot.unplaced = vec![
        found(
            "studio",
            "studio",
            Some(Os::Macos),
            UnplacedState::Ready,
            Via::Mdns,
            Some(STUDIO),
        ),
        found(
            "living",
            "living-room",
            None,
            UnplacedState::Identifying,
            Via::Mdns,
            None,
        ),
        found(
            "gaming",
            "gaming-pc",
            Some(Os::Windows),
            UnplacedState::DifferentVersion,
            Via::Mdns,
            Some(GAMING),
        ),
        found(
            "tower",
            "tower",
            Some(Os::Linux),
            UnplacedState::DuplicateName,
            Via::Address,
            Some(TOWER),
        ),
    ];
    snapshot.pairing_window = View {
        state: State::Open,
        seconds_left: Some(540),
        holding: None,
        reason: None,
    };
    find(snapshot, window, cx)
}

fn find_holding(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = alone();
    snapshot.unplaced = vec![studio()];
    snapshot.pairing_window = View {
        state: State::Open,
        seconds_left: Some(420),
        holding: Some(Holding {
            name: "studio".into(),
            mark: STUDIO.into(),
            ms_left: 3000,
        }),
        reason: None,
    };
    find(snapshot, window, cx)
}

fn find_copied(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = alone();
    snapshot.unplaced = vec![
        studio(),
        found(
            "kitchen",
            "kitchen-mac",
            Some(Os::Macos),
            UnplacedState::Ready,
            Via::Mdns,
            Some(KITCHEN),
        ),
    ];
    snapshot.pairing_window = View {
        state: State::Closed,
        seconds_left: None,
        holding: None,
        reason: Some(Reason::Rival),
    };
    app_window(snapshot, WINDOW, window, cx, |nid, _, _| nid.copied = true)
}

fn studio() -> nid::neighbors::Unplaced {
    found(
        "studio",
        "studio",
        Some(Os::Macos),
        UnplacedState::Ready,
        Via::Mdns,
        Some(STUDIO),
    )
}

fn step(
    snapshot: nid_app::model::Snapshot,
    step: Step,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    app_window(snapshot, WINDOW, window, cx, move |nid, _, _| {
        nid.first_run = Some(step);
    })
}

fn placing() -> Step {
    Step::Place {
        id: "key:studio".into(),
        name: "studio".into(),
        mark: Some(STUDIO.into()),
    }
}

fn place(window: &mut Window, cx: &mut App) -> AnyView {
    let mut snapshot = alone();
    snapshot.unplaced = vec![studio()];
    step(snapshot, placing(), window, cx)
}

fn place_placed(window: &mut Window, cx: &mut App) -> AnyView {
    step(one_peer(PeerState::Connected), placing(), window, cx)
}

fn place_not_answering(window: &mut Window, cx: &mut App) -> AnyView {
    step(one_peer(PeerState::Connecting), placing(), window, cx)
}

fn trying(reached: bool, back: bool) -> Step {
    Step::Try {
        name: "studio".into(),
        reached,
        back,
    }
}

fn try_waiting(window: &mut Window, cx: &mut App) -> AnyView {
    step(
        one_peer(PeerState::Connecting),
        trying(false, false),
        window,
        cx,
    )
}

fn try_reached(window: &mut Window, cx: &mut App) -> AnyView {
    step(
        one_peer(PeerState::Connected),
        trying(true, false),
        window,
        cx,
    )
}

fn try_back(window: &mut Window, cx: &mut App) -> AnyView {
    step(
        one_peer(PeerState::Connected),
        trying(true, true),
        window,
        cx,
    )
}

fn added_by(window: &mut Window, cx: &mut App) -> AnyView {
    app_window(
        one_peer(PeerState::Connected),
        WINDOW,
        window,
        cx,
        |nid, _, cx| {
            nid.model.update(cx, |model, _| {
                model.added_by = Some(fixtures::joined("studio", STUDIO));
            });
        },
    )
}

fn address(
    typed: &'static str,
    error: Option<&'static str>,
    looking: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    app_window(alone(), WINDOW, window, cx, move |_, window, cx| {
        let view = cx.entity();
        after_first_frame(window, move |window, cx| {
            view.update(cx, |nid, cx| {
                nid.open_add_address(window, cx);
                nid.address
                    .update(cx, |input, cx| input.set_value(typed, window, cx));
                nid.address_error = error.map(str::to_owned);
                nid.looking_up = looking;
                cx.notify();
            });
        });
    })
}

fn add_address(window: &mut Window, cx: &mut App) -> AnyView {
    address("", None, false, window, cx)
}

fn add_address_error(window: &mut Window, cx: &mut App) -> AnyView {
    address(
        "127.0.0.1",
        Some("127.0.0.1:24800 cannot be added: that address is this computer"),
        false,
        window,
        cx,
    )
}

fn add_address_looking(window: &mut Window, cx: &mut App) -> AnyView {
    address("100.64.0.7", None, true, window, cx)
}
