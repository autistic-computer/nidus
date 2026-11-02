use ala::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div,
};
use nid::app::api::{Level, PeerState};
use nid_app::{
    model::{Page, Requests, Snapshot},
    pages::first_run::Step,
    window::NidWindow,
};
use penna::ActiveTheme as _;
use penna_storybook::{Section, Story, story::states};

use crate::fixtures::{self, STUDIO, alone, beside, health, nid_window, one_peer, settled};

pub const STORY: Story = Story {
    id: "titlebar",
    name: "Titlebar",
    group: "Nidus",
    summary: "The window's titlebar: where the keyboard is and which computer is beside you, the first-run steps, the settings title and the sharing switch. Each row is the real window cut to its titlebar.",
    sections: &[
        Section {
            id: "status",
            name: "Status pill",
            summary: "The pill for each state; while ready, it names the computer beside you and the side it sits on.",
            build: status,
        },
        Section {
            id: "first-run-steps",
            name: "First-run steps",
            summary: "The steps that stand in for the pill during the first run.",
            build: steps,
        },
        Section {
            id: "settings-title",
            name: "Settings title",
            summary: "On the settings page the pill gives way to its title and the gear becomes a close button.",
            build: settings_title,
        },
        Section {
            id: "sharing-switch",
            name: "Sharing switch",
            summary: "On, off, and disabled while the service does not say.",
            build: sharing,
        },
    ],
};

type Setup = Box<dyn FnOnce(&mut NidWindow, &mut Window, &mut Context<NidWindow>)>;

struct Titlebars {
    rows: Vec<(SharedString, AnyView)>,
    _requests: Vec<Requests>,
}

impl Render for Titlebars {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = cx.theme().title_bar_height();
        self.rows.iter().fold(states(), |states, (label, view)| {
            states.state(
                label.clone(),
                div()
                    .w_full()
                    .h(height)
                    .overflow_hidden()
                    .child(view.clone()),
            )
        })
    }
}

fn titlebars(
    rows: Vec<(&'static str, Snapshot, Setup)>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let mut views = Vec::new();
    let mut requests = Vec::new();
    for (label, snapshot, setup) in rows {
        let (view, queue) = nid_window(snapshot, window, cx, setup);
        views.push((SharedString::from(label), AnyView::from(view)));
        requests.push(queue);
    }
    cx.new(|_| Titlebars {
        rows: views,
        _requests: requests,
    })
    .into()
}

fn plain() -> Setup {
    Box::new(|_, _, _| {})
}

fn attention() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.health = vec![health(
        "desktop",
        Level::Error,
        "Desktop helper",
        "The desktop helper is not running.",
        None,
    )];
    settled(snapshot)
}

fn paused() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.sharing = Some(false);
    settled(snapshot)
}

fn status(window: &mut Window, cx: &mut App) -> AnyView {
    titlebars(
        vec![
            (
                "ready, beside on the right",
                one_peer(PeerState::Connected),
                plain(),
            ),
            (
                "ready, beside on the left",
                beside(PeerState::Connected, (-1920, 0)),
                plain(),
            ),
            (
                "ready, above",
                beside(PeerState::Connected, (0, -1080)),
                plain(),
            ),
            (
                "ready, below",
                beside(PeerState::Connected, (0, 1080)),
                plain(),
            ),
            (
                "ready, nothing beside",
                beside(PeerState::Connected, (2520, 400)),
                plain(),
            ),
            (
                "controlling",
                one_peer(PeerState::ControlledFromHere),
                plain(),
            ),
            ("controlled", one_peer(PeerState::ControllingThis), plain()),
            ("paused", paused(), plain()),
            ("checking", one_peer(PeerState::Connecting), plain()),
            ("needs attention", attention(), plain()),
        ],
        window,
        cx,
    )
}

fn steps(window: &mut Window, cx: &mut App) -> AnyView {
    titlebars(
        vec![
            ("find", alone(), plain()),
            (
                "place",
                alone(),
                Box::new(|nid, _, _| {
                    nid.first_run = Some(Step::Place {
                        id: "key:studio".into(),
                        name: "studio".into(),
                        mark: Some(STUDIO.into()),
                    });
                }),
            ),
            (
                "added by another computer",
                one_peer(PeerState::Connected),
                Box::new(|nid, _, cx| {
                    nid.model.update(cx, |model, _| {
                        model.added_by = Some(fixtures::joined("studio", STUDIO));
                    });
                }),
            ),
        ],
        window,
        cx,
    )
}

fn settings_title(window: &mut Window, cx: &mut App) -> AnyView {
    titlebars(
        vec![(
            "settings",
            one_peer(PeerState::Connected),
            Box::new(|nid, _, cx| {
                nid.model
                    .update(cx, |model, cx| model.show(Page::Settings, cx));
            }),
        )],
        window,
        cx,
    )
}

fn sharing(window: &mut Window, cx: &mut App) -> AnyView {
    let mut unknown = one_peer(PeerState::Connected);
    unknown.sharing = None;
    titlebars(
        vec![
            ("on", one_peer(PeerState::Connected), plain()),
            ("off", paused(), plain()),
            ("disabled", settled(unknown), plain()),
        ],
        window,
        cx,
    )
}
