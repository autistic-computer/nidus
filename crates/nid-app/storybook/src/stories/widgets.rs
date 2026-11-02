use ala::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window, px};
use nid::{
    app::api::{Level, PeerState, State},
    pairing_window::{Holding, Reason, State as Pairing, View},
};
use nid_app::{copy, widgets};
use penna::{Dot, Label, TextStyle, button::Button, h_flex};
use penna_storybook::{
    Section, Story,
    story::{row, stateless, states},
};

use crate::fixtures::{GAMING, KITCHEN, OWN, STUDIO, TOWER};

pub const STORY: Story = Story {
    id: "widgets",
    name: "Widgets",
    group: "Nidus",
    summary: "The small pieces every page shares: health banners, notes, the colored marks that tell two computers apart, and the dots and tones that say how a computer is doing.",
    sections: &[
        Section {
            id: "banners",
            name: "Banners",
            summary: "A health row as a banner, for each level, with and without a fix button.",
            build: |_, cx| stateless(banners, cx),
        },
        Section {
            id: "notes",
            name: "Pairing notes",
            summary: "The note under the computers found nearby, for each state of this computer's pairing window.",
            build: |_, cx| stateless(notes, cx),
        },
        Section {
            id: "key-marks",
            name: "Key marks",
            summary: "The first four digits of a key pick four colors, the same on every computer and in every theme.",
            build: |_, cx| stateless(marks, cx),
        },
        Section {
            id: "peer-dots",
            name: "Computer dots",
            summary: "The dot beside a computer's name for each state it can be in.",
            build: |_, cx| stateless(dots, cx),
        },
        Section {
            id: "state-tones",
            name: "Status tones",
            summary: "The tone of the status dot for each state of this computer.",
            build: |_, cx| stateless(tones, cx),
        },
    ],
};

fn fix(id: &'static str, level: Level, label: &'static str, cx: &App) -> AnyElement {
    Button::new(id)
        .on(widgets::banner_ground(level, cx))
        .label(label)
        .into_any_element()
}

fn banners(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "ok",
            widgets::banner(Level::Ok, "Paired computers", "No problems found.", None),
        )
        .state(
            "warning",
            widgets::banner(
                Level::Warning,
                "Nearby computers",
                "This network blocks discovery. Add computers by address instead.",
                None,
            ),
        )
        .state(
            "warning with a fix",
            widgets::banner(
                Level::Warning,
                "Paired computers",
                "studio refused the connection: its clock is 9 minutes off.",
                Some(fix("retry", Level::Warning, "Retry", cx)),
            ),
        )
        .state(
            "error with a fix",
            widgets::banner(
                Level::Error,
                "Desktop helper",
                "The desktop helper is not running, so Nidus cannot move the pointer here.",
                Some(fix("install", Level::Error, "Install…", cx)),
            ),
        )
        .into_any_element()
}

fn notes(_: &mut Window, _: &mut App) -> AnyElement {
    let open = |seconds: u64| View {
        state: Pairing::Open,
        seconds_left: Some(seconds),
        holding: None,
        reason: None,
    };
    let closed = |reason| View {
        state: Pairing::Closed,
        seconds_left: None,
        holding: None,
        reason: Some(reason),
    };
    [
        ("open", open(540)),
        ("open, last minute", open(40)),
        (
            "open, accepting one by itself",
            View {
                holding: Some(Holding {
                    name: "studio".into(),
                    mark: STUDIO.into(),
                    ms_left: 3000,
                }),
                ..open(420)
            },
        ),
        ("closed, two showed up", closed(Reason::Rival)),
        ("closed, adding failed", closed(Reason::Failed)),
    ]
    .into_iter()
    .fold(states(), |states, (label, view)| {
        match copy::finish_note(&view, Some("studio"), "laptop") {
            Some((title, detail)) => states.state(label, widgets::note(title, detail)),
            None => states,
        }
    })
    .into_any_element()
}

fn marks(_: &mut Window, _: &mut App) -> AnyElement {
    let sizes = [7.0, 10.0, 14.0, 20.0]
        .into_iter()
        .fold(states().columns(4), |states, size| {
            states.state(format!("{size} px"), widgets::key_mark_sized(OWN, size))
        });
    states()
        .state(
            "marks",
            row().children([OWN, STUDIO, TOWER, KITCHEN, GAMING].map(|mark| {
                h_flex()
                    .gap(px(8.0))
                    .pr(px(12.0))
                    .child(widgets::key_mark(mark))
                    .child(Label::new(TextStyle::Mono, mark).muted())
            })),
        )
        .state("sizes", sizes)
        .into_any_element()
}

fn dots(_: &mut Window, _: &mut App) -> AnyElement {
    let entry = |dot: Dot, text: &'static str| {
        h_flex()
            .gap(px(8.0))
            .child(dot)
            .child(Label::new(TextStyle::Body, text))
    };
    [
        PeerState::Paired,
        PeerState::Connecting,
        PeerState::Connected,
        PeerState::ControllingThis,
        PeerState::ControlledFromHere,
        PeerState::Unreachable,
    ]
    .into_iter()
    .fold(
        states()
            .columns(3)
            .state("None", entry(widgets::state_dot(None), copy::THIS_COMPUTER)),
        |states, state| {
            states.state(
                format!("{state:?}"),
                entry(widgets::state_dot(Some(state)), copy::peer_state(state)),
            )
        },
    )
    .into_any_element()
}

fn tones(_: &mut Window, _: &mut App) -> AnyElement {
    [
        (State::Ready, "Ready"),
        (State::Controlling, "On another computer"),
        (State::Controlled, "Used from another computer"),
        (State::Paused, "Paused"),
        (State::Checking, "Checking…"),
        (State::Setup, "Add a computer"),
        (State::Attention, "Needs attention"),
    ]
    .into_iter()
    .fold(states().columns(4), |states, (state, text)| {
        states.state(
            format!("{state:?}"),
            h_flex()
                .gap(px(8.0))
                .child(Dot::new(widgets::state_tone(state)))
                .child(Label::new(TextStyle::Body, text)),
        )
    })
    .into_any_element()
}
