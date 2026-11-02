use std::{cell::Cell, rc::Rc};

use ala::{
    AnyView, App, AppContext as _, Bounds, Context, Entity, IntoElement, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels, PlatformInput, Point, Render,
    Styled as _, Window, canvas, div, point, px,
};
use nid::app::api::PeerState;
use nid_app::{
    board::{
        Board,
        geometry::{DRAG_TOLERANCE, Fit},
    },
    model::{Requests, Snapshot},
};
use penna_storybook::{Section, Story};

use crate::fixtures::{self, BOARD, frame, many_screens, nearby, one_peer, settled, two_peers};

const DOCKED: f32 = 580.0;

pub const STORY: Story = Story {
    id: "board",
    name: "Board",
    group: "Nidus",
    summary: "The desk on its own: every computer's screens to scale, the crossings lit between them, a chip naming each computer and the dock of computers found nearby.",
    sections: &[
        Section {
            id: "detecting",
            name: "Detecting",
            summary: "No layout yet.",
            build: |_, cx| desk(no_layout(), false, cx, |_, _| {}),
        },
        Section {
            id: "one-peer",
            name: "One computer",
            summary: "Two screens side by side and the crossing between them.",
            build: |_, cx| desk(one_peer(PeerState::Connected), false, cx, |_, _| {}),
        },
        Section {
            id: "two-peers-two-screens",
            name: "Screens on every side",
            summary: "This computer with two screens, one computer to the right and one with two screens below. Screens of the same computer meet at a dotted joint.",
            build: |_, cx| desk(many_screens(), false, cx, |_, _| {}),
        },
        Section {
            id: "selected",
            name: "Selected",
            summary: "A selected computer is outlined in the accent and its screens are named.",
            build: |_, cx| {
                desk(many_screens(), false, cx, |board, cx| {
                    board.select(Some("peer:studio".into()), cx);
                })
            },
        },
        Section {
            id: "adjusting",
            name: "Adjusting",
            summary: "Moving one computer's screens one by one: a dashed outline and Done beside its chip.",
            build: |_, cx| {
                desk(many_screens(), false, cx, |board, cx| {
                    board.adjust("peer:tower", true, cx);
                })
            },
        },
        Section {
            id: "controlling",
            name: "Controlling",
            summary: "The pointer is over on the other computer: its screen takes the primary tone and this computer's is dimmed.",
            build: |_, cx| {
                desk(
                    one_peer(PeerState::ControlledFromHere),
                    false,
                    cx,
                    |_, _| {},
                )
            },
        },
        Section {
            id: "offline",
            name: "Offline",
            summary: "A computer that cannot be reached: its screen is dimmed and the crossing to it is muted.",
            build: |_, cx| desk(two_peers(), false, cx, |_, _| {}),
        },
        Section {
            id: "sharing-off",
            name: "Sharing off",
            summary: "Crossings are dimmed while sharing is off.",
            build: |_, cx| desk(sharing_off(), false, cx, |_, _| {}),
        },
        Section {
            id: "dock-found",
            name: "Dock",
            summary: "Computers found nearby, docked under the desk; the one on a different version cannot be dragged.",
            build: |_, cx| desk(with_nearby(), true, cx, |_, _| {}),
        },
        Section {
            id: "dragging",
            name: "Dragging",
            summary: "A computer held mid-drag. Nidus answered where it lands, so it snaps there and shows the crossing it would get.",
            build: dragging,
        },
    ],
};

fn desk(
    snapshot: Snapshot,
    docked: bool,
    cx: &mut App,
    setup: impl FnOnce(&mut Board, &mut Context<Board>),
) -> AnyView {
    let (board, requests) = fixtures::board(snapshot, docked, cx, setup);
    let height = if docked { DOCKED } else { BOARD };
    frame(board.into(), None, height, vec![requests], cx)
}

fn no_layout() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.layout = None;
    snapshot.seams.clear();
    snapshot
}

fn sharing_off() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.sharing = Some(false);
    settled(snapshot)
}

fn with_nearby() -> Snapshot {
    let mut snapshot = one_peer(PeerState::Connected);
    snapshot.unplaced = nearby();
    snapshot
}

struct Dragging {
    board: Entity<Board>,
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    requests: Requests,
}

impl Render for Dragging {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let area = self.area.clone();
        div()
            .relative()
            .w_full()
            .h(px(BOARD))
            .child(self.board.clone())
            .child(
                canvas(move |bounds, _, _| area.set(Some(bounds)), |_, (), _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

fn dragging(window: &mut Window, cx: &mut App) -> AnyView {
    let snapshot = one_peer(PeerState::Connected);
    let layout = snapshot.layout.clone().unwrap_or_default();
    let (board, requests) = fixtures::board(snapshot, false, cx, |_, _| {});
    let area = Rc::new(Cell::new(None));
    let view = cx.new(|_| Dragging {
        board,
        area: area.clone(),
        requests,
    });
    let held = view.clone();
    let then: Then = Box::new(move |area, window, cx| {
        let Some(fit) = Fit::new(
            layout.monitors.iter(),
            f32::from(area.size.width),
            f32::from(area.size.height),
        ) else {
            return;
        };
        let Some(studio) = layout
            .monitors
            .iter()
            .find(|monitor| monitor.peer.is_some())
        else {
            return;
        };
        let tile = fit.rect(studio, (0.0, 0.0));
        let from = area.origin
            + point(
                px(tile.x + tile.width / 2.0),
                px(tile.y + tile.height / 2.0),
            );
        let by = (6.0, 540.0 * fit.scale);
        let to = from + point(px(by.0), px(by.1));
        let zoom = window.zoom();
        let at = |position: Point<Pixels>| position * zoom;
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(from),
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(to),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }),
            cx,
        );
        let landing = layout.preview(
            &studio.id,
            studio.x + fit.units(by.0),
            studio.y + fit.units(by.1),
            fit.units(DRAG_TOLERANCE).unsigned_abs(),
            true,
        );
        held.update(cx, |dragging, _| {
            dragging
                .requests
                .next(serde_json::to_value(landing).unwrap_or_default());
        });
    });
    when_laid_out(area, None, 0, 240, window, then);
    view.into()
}

type Then = Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>;

fn when_laid_out(
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    last: Option<Bounds<Pixels>>,
    steady: u32,
    tries: u32,
    window: &Window,
    then: Then,
) {
    window.on_next_frame(move |window, cx| {
        let current = area.get().filter(|bounds| bounds.size.width > px(0.0));
        let steady = if current.is_some() && current == last {
            steady + 1
        } else {
            0
        };
        match current {
            Some(bounds) if steady >= 3 => then(bounds, window, cx),
            _ if tries > 0 => when_laid_out(area, current, steady, tries - 1, window, then),
            _ => {}
        }
    });
}
