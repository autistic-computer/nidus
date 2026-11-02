use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use nid::{
    app::{
        api::{self, Action, Health, Level, Peer, PeerState, Shortcut, Status},
        layout_model::{Layout, Monitor},
    },
    core::KeyboardMode,
    desktop::{Display, Rect},
    hello::{Notice, NoticeKind},
    neighbors::{Unplaced, UnplacedState, Via},
    pairing_window::View,
    wire::Os,
};
use nid_app::{
    board::Board,
    copy,
    model::{Model, Requests, Snapshot},
    panel::Panel,
    platform,
    window::NidWindow,
};

pub const OWN: &str = "f04ee5";
pub const STUDIO: &str = "996615";
pub const TOWER: &str = "a4d876";
pub const KITCHEN: &str = "3c71b0";
pub const GAMING: &str = "5e0f2a";

pub const WINDOW: f32 = 680.0;
pub const BOARD: f32 = 440.0;
const PANEL_WIDTH: f32 = 380.0;

fn owner(peer: Option<&str>) -> (String, String) {
    peer.map_or_else(
        || ("local".to_owned(), copy::THIS_COMPUTER.to_owned()),
        |name| (format!("peer:{name}"), name.to_owned()),
    )
}

pub fn screen(peer: Option<&str>, x: i32, y: i32) -> Monitor {
    let (id, label) = owner(peer);
    Monitor {
        id,
        label,
        peer: peer.map(str::to_owned),
        display: None,
        x,
        y,
        width: 1920,
        height: 1080,
    }
}

pub fn display(
    peer: Option<&str>,
    (id, name): (&str, &str),
    (x, y): (i32, i32),
    (width, height): (u32, u32),
    (width_mm, height_mm): (u32, u32),
) -> Monitor {
    let (owner, label) = owner(peer);
    Monitor {
        id: format!("{owner}/{id}"),
        label: format!("{name} · {label}"),
        peer: peer.map(str::to_owned),
        display: Some(Display {
            id: id.into(),
            name: name.into(),
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
            width_mm,
            height_mm,
            active: true,
        }),
        x,
        y,
        width,
        height,
    }
}

pub fn peer(name: &str, state: PeerState, mark: &str) -> Peer {
    Peer {
        name: name.into(),
        state,
        detail: copy::peer_state(state).into(),
        allow_control: true,
        keyboard: KeyboardMode::Standard,
        reverse_scroll: false,
        share_screen: false,
        mark: mark.into(),
    }
}

pub fn found(
    id: &str,
    name: &str,
    os: Option<Os>,
    state: UnplacedState,
    via: Via,
    mark: Option<&str>,
) -> Unplaced {
    Unplaced {
        id: format!("key:{id}"),
        name: name.into(),
        os,
        mark: mark.map(str::to_owned),
        version: Some(if state == UnplacedState::DifferentVersion {
            "0.6.2".into()
        } else {
            env!("CARGO_PKG_VERSION").into()
        }),
        state,
        via,
    }
}

pub fn health(
    id: &str,
    level: Level,
    title: &str,
    detail: &str,
    fix: Option<(&str, &str)>,
) -> Health {
    Health {
        action: fix.map(|(label, command)| Action {
            label: label.into(),
            command: command.into(),
        }),
        ..Health::new(id, level, title, detail)
    }
}

pub fn shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut {
            title: "Switch to the other computer".into(),
            keys: "Ctrl+Super+Space".into(),
        },
        Shortcut {
            title: "Return input to this computer".into(),
            keys: copy::RETURN_KEYS.into(),
        },
    ]
}

pub fn joined(name: &str, mark: &str) -> Notice {
    Notice {
        id: 1,
        kind: NoticeKind::Joined,
        name: name.into(),
        mark: mark.into(),
    }
}

pub fn desk(peers: Vec<Peer>, monitors: Vec<Monitor>) -> Snapshot {
    let layout = Layout { monitors };
    settled(api::Snapshot {
        status: Status::new(None, &[], &[], false),
        sharing: Some(true),
        health: Vec::new(),
        pairing_window: View::default(),
        seams: layout.seams(),
        layout: Some(layout),
        own_mark: Some(OWN.into()),
        unplaced: Vec::new(),
        peers,
        pause_at_edges: Some(false),
        shortcuts: Vec::new(),
        share_clipboard: Some(true),
        autostart: Some(true),
        config_path: "/home/you/.config/nid/nid.toml".into(),
        notices: Vec::new(),
        platform: None,
    })
}

pub fn settled(mut snapshot: Snapshot) -> Snapshot {
    let linked = snapshot.peers.iter().any(|peer| copy::linked(peer.state));
    let checking = !linked
        && snapshot
            .peers
            .iter()
            .any(|peer| peer.state == PeerState::Connecting);
    snapshot.status = Status::new(
        snapshot.sharing,
        &snapshot.peers,
        &snapshot.health,
        checking,
    );
    snapshot
}

pub fn alone() -> Snapshot {
    desk(Vec::new(), vec![screen(None, 0, 0)])
}

pub fn one_peer(state: PeerState) -> Snapshot {
    beside(state, (1920, 0))
}

pub fn beside(state: PeerState, (x, y): (i32, i32)) -> Snapshot {
    desk(
        vec![peer("studio", state, STUDIO)],
        vec![screen(None, 0, 0), screen(Some("studio"), x, y)],
    )
}

pub fn two_peers() -> Snapshot {
    desk(
        vec![
            peer("studio", PeerState::Connected, STUDIO),
            peer("tower", PeerState::Unreachable, TOWER),
        ],
        vec![
            screen(None, 0, 0),
            screen(Some("studio"), 1920, 0),
            screen(Some("tower"), -1920, 0),
        ],
    )
}

pub fn many_screens() -> Snapshot {
    desk(
        vec![
            peer("studio", PeerState::Connected, STUDIO),
            peer("tower", PeerState::Connected, TOWER),
        ],
        vec![
            display(
                None,
                ("eDP-1", "Built-in display"),
                (0, 240),
                (1920, 1200),
                (302, 189),
            ),
            display(
                None,
                ("DP-1", "U2723QE"),
                (1920, 0),
                (2560, 1440),
                (597, 336),
            ),
            display(
                Some("studio"),
                ("1", "Studio Display"),
                (4480, 0),
                (2560, 1440),
                (597, 336),
            ),
            display(
                Some("tower"),
                ("HDMI-A-1", "27GN950"),
                (0, 1440),
                (1920, 1080),
                (600, 340),
            ),
            display(
                Some("tower"),
                ("DP-2", "VG27AQ"),
                (1920, 1440),
                (2560, 1440),
                (597, 336),
            ),
        ],
    )
}

pub fn nearby() -> Vec<Unplaced> {
    vec![
        found(
            "kitchen",
            "kitchen-mac",
            Some(Os::Macos),
            UnplacedState::Ready,
            Via::Mdns,
            Some(KITCHEN),
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
            "lab",
            "lab",
            Some(Os::Linux),
            UnplacedState::Ready,
            Via::Address,
            None,
        ),
    ]
}

pub fn after_first_frame(window: &Window, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    window.on_next_frame(move |window, _| window.on_next_frame(f));
}

pub struct Frame {
    view: AnyView,
    width: Option<f32>,
    height: f32,
    _requests: Vec<Requests>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .map(|this| match self.width {
                Some(width) => this.w(px(width)),
                None => this.w_full(),
            })
            .h(px(self.height))
            .child(self.view.clone())
    }
}

pub fn frame(
    view: AnyView,
    width: Option<f32>,
    height: f32,
    requests: Vec<Requests>,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| Frame {
        view,
        width,
        height,
        _requests: requests,
    })
    .into()
}

pub fn nid_window(
    snapshot: Snapshot,
    window: &mut Window,
    cx: &mut App,
    setup: impl FnOnce(&mut NidWindow, &mut Window, &mut Context<NidWindow>),
) -> (Entity<NidWindow>, Requests) {
    let (model, requests) = Model::fixture(snapshot);
    let model = cx.new(|_| model);
    let services = platform::init(cx);
    let view = cx.new(|cx| NidWindow::new(model, services, window, cx));
    view.update(cx, |nid, cx| setup(nid, window, cx));
    (view, requests)
}

pub fn app_window(
    snapshot: Snapshot,
    height: f32,
    window: &mut Window,
    cx: &mut App,
    setup: impl FnOnce(&mut NidWindow, &mut Window, &mut Context<NidWindow>),
) -> AnyView {
    let (view, requests) = nid_window(snapshot, window, cx, setup);
    frame(view.into(), None, height, vec![requests], cx)
}

pub fn panel(snapshot: Option<Snapshot>, cx: &mut App) -> AnyView {
    let rows = snapshot.as_ref().map_or(0, |snapshot| {
        snapshot.peers.len()
            + snapshot
                .unplaced
                .iter()
                .filter(|computer| copy::placeable(computer))
                .count()
    });
    let height = (300.0 + 40.0 * rows.max(1) as f32).clamp(340.0, 560.0);
    let (mut model, requests) = Model::fixture(snapshot.clone().unwrap_or_else(alone));
    model.snapshot = snapshot;
    let model = cx.new(|_| model);
    let view = cx.new(|cx| Panel::new(model, cx));
    frame(view.into(), Some(PANEL_WIDTH), height, vec![requests], cx)
}

pub fn board(
    snapshot: Snapshot,
    docked: bool,
    cx: &mut App,
    setup: impl FnOnce(&mut Board, &mut Context<Board>),
) -> (Entity<Board>, Requests) {
    let (model, requests) = Model::fixture(snapshot);
    let model = cx.new(|_| model);
    let board = cx.new(|cx| Board::new(model, cx));
    board.update(cx, |board, cx| {
        board.set_docked(docked, cx);
        setup(board, cx);
    });
    (board, requests)
}
