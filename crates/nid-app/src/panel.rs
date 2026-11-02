#![cfg_attr(
    target_os = "linux",
    allow(
        dead_code,
        reason = "Linux keeps the tray menu; the panel is for the menu bar and the notification area"
    )
)]

use std::time::{Duration, Instant};

use ala::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, div, point, prelude::FluentBuilder as _, px, size,
};
use nid::app::api::{Level, PeerState, Request, State};
use penna::{
    Label, Notice, Root, Sizable as _, Space, TextStyle, Tone,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Keycap,
    list::ListItem,
    separator::Separator,
    switch::Switch,
    tray::TrayAnchor,
    tray::TrayPanel,
    v_flex,
};

use crate::{
    board, copy,
    model::{Model, Page, Snapshot},
    platform, widgets,
};

const WIDTH: f32 = 380.0;
const ROW: f32 = 40.0;
const GAP: f32 = 6.0;
const EDGE: f32 = 8.0;
const MAX_HEIGHT: f32 = 560.0;
const REOPEN: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Ready,
    Away,
    Visited,
    Paused,
    Attention,
    Update,
}

pub fn look(snapshot: Option<&Snapshot>, update: bool) -> Look {
    match snapshot.map(|snapshot| snapshot.status.state) {
        Some(State::Controlling) => Look::Away,
        Some(State::Controlled) => Look::Visited,
        _ if update => Look::Update,
        Some(State::Ready) => Look::Ready,
        Some(State::Paused | State::Checking) => Look::Paused,
        Some(State::Setup | State::Attention) | None => Look::Attention,
    }
}

pub fn place(icon: Bounds<Pixels>, area: Bounds<Pixels>, height: f32) -> Bounds<Pixels> {
    let (left, right) = (f32::from(area.left()), f32::from(area.right()));
    let x = (f32::from(icon.center().x) - WIDTH / 2.0)
        .clamp(left + EDGE, (right - WIDTH - EDGE).max(left + EDGE));
    let y = if icon.center().y < area.center().y {
        f32::from(icon.bottom()) + GAP
    } else {
        f32::from(icon.top()) - height - GAP
    };
    Bounds::new(point(px(x), px(y)), size(px(WIDTH), px(height)))
}

fn height(snapshot: Option<&Snapshot>) -> f32 {
    let rows = snapshot.map_or(0, |snapshot| {
        snapshot.peers.len()
            + snapshot
                .unplaced
                .iter()
                .filter(|computer| copy::placeable(computer))
                .count()
    });
    (300.0 + ROW * rows.max(1) as f32).min(MAX_HEIGHT)
}

#[derive(Default)]
struct Shown {
    window: Option<WindowHandle<Root>>,
    closed: Option<Instant>,
}

impl ala::Global for Shown {}

pub fn toggle(anchor: Option<TrayAnchor>, cx: &mut App) {
    let shown = cx.default_global::<Shown>();
    let open = shown.window.take();
    let just_closed = shown.closed.take().is_some_and(|at| at.elapsed() < REOPEN);
    if let Some(window) = open
        && window
            .update(cx, |_, window, _| window.remove_window())
            .is_ok()
    {
        return;
    }
    if just_closed {
        return;
    }
    let Some(anchor) = anchor else {
        crate::open(cx);
        return;
    };
    let model = crate::model::model(cx);
    let area = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == anchor.display)
        .map_or(anchor.bounds, |display| display.visible_bounds());
    let bounds = place(
        anchor.bounds,
        area,
        height(model.read(cx).snapshot.as_ref()),
    );
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: Some(anchor.display),
            titlebar: None,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            focus: true,
            show: true,
            app_id: Some(crate::APP_ID.into()),
            ..Default::default()
        },
        |window, cx| {
            crate::theme::apply(Some(window), cx);
            let view = cx.new(|cx| Panel::popup(model, (anchor.bounds, area), window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    );
    match opened {
        Ok(handle) => cx.default_global::<Shown>().window = Some(handle),
        Err(error) => {
            log::error!("Nidus could not open its panel: {error:#}");
            crate::open(cx);
        }
    }
}

fn close(window: &mut Window, cx: &mut App) {
    window.remove_window();
    let shown = cx.default_global::<Shown>();
    shown.window = None;
    shown.closed = Some(Instant::now());
}

fn go(page: Page) -> impl Fn(&mut Panel, &ala::ClickEvent, &mut Window, &mut Context<Panel>) {
    move |panel, _, window, cx| {
        panel
            .model
            .update(cx, |model, cx| model.show(page.clone(), cx));
        if panel.popup.is_some() {
            close(window, cx);
            crate::open(cx);
        }
    }
}

pub struct Panel {
    model: Entity<Model>,
    focus: FocusHandle,
    was_active: bool,
    popup: Option<(Bounds<Pixels>, Bounds<Pixels>)>,
    _observers: Vec<Subscription>,
}

impl Panel {
    pub fn new(model: Entity<Model>, cx: &mut Context<Self>) -> Self {
        Self {
            _observers: vec![cx.observe(&model, |_, _, cx| cx.notify())],
            model,
            focus: cx.focus_handle(),
            was_active: false,
            popup: None,
        }
    }

    fn popup(
        model: Entity<Model>,
        anchor: (Bounds<Pixels>, Bounds<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self::new(model, cx);
        panel.popup = Some(anchor);
        window.focus(&panel.focus, cx);
        panel
            ._observers
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.was_active = true;
                } else if this.was_active {
                    close(window, cx);
                }
            }));
        panel
    }

    fn send(&self, request: Request, cx: &App) {
        self.model.read(cx).send(request);
    }
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.model.read(cx).snapshot.clone();
        let panel = TrayPanel::new()
            .h_auto()
            .max_h(px(MAX_HEIGHT))
            .child(header(snapshot.as_ref(), cx))
            .map(|panel| match &snapshot {
                Some(snapshot) => panel.child(status(snapshot, cx)).child(
                    div()
                        .id("panel-computers")
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(computers(snapshot, cx)),
                ),
                None => panel.child(Label::new(TextStyle::Body, "Starting…").muted()),
            })
            .child(footer(cx));
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.popup.is_some() && event.keystroke.key == "escape" {
                    close(window, cx);
                }
            }))
            .when_some(self.popup, |this, (icon, area)| {
                this.on_children_prepainted(move |children, window, _| {
                    let Some(panel) = children.first() else {
                        return;
                    };
                    let fitted = place(icon, area, f32::from(panel.size.height));
                    if (window.bounds().size.height - fitted.size.height).abs() > px(0.5) {
                        window.set_bounds(fitted);
                    }
                })
            })
            .child(panel)
    }
}

fn header(snapshot: Option<&Snapshot>, cx: &mut Context<Panel>) -> AnyElement {
    let sharing = snapshot
        .filter(|snapshot| !snapshot.peers.is_empty())
        .and_then(|snapshot| snapshot.sharing);
    h_flex()
        .gap(px(8.0))
        .child(Label::new(TextStyle::Heading, "Nidus").flex_1())
        .when_some(sharing, |this, sharing| {
            this.child(Label::new(TextStyle::Body, "Sharing").muted())
                .child(
                    Switch::new("panel-sharing")
                        .checked(sharing)
                        .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                            this.send(Request::SetSharing { enabled: *enabled }, cx);
                        })),
                )
        })
        .into_any_element()
}

fn status(snapshot: &Snapshot, cx: &mut Context<Panel>) -> AnyElement {
    let state = snapshot.status.state;
    let peer = snapshot
        .status
        .peer
        .clone()
        .unwrap_or_else(|| "the other computer".to_owned());
    let problem = snapshot.health.iter().find(|row| row.level != Level::Ok);
    let (tone, title, detail, action): (Tone, String, String, Option<AnyElement>) = match state {
        State::Controlling => (
            Tone::Primary,
            format!("On {peer}"),
            format!("Your keyboard and pointer are working on {peer}."),
            Some(
                h_flex()
                    .gap(px(8.0))
                    .child(Keycap::new(copy::RETURN_KEYS))
                    .child(Label::new(TextStyle::Small, "brings them back").muted())
                    .into_any_element(),
            ),
        ),
        State::Controlled => (
            Tone::Primary,
            format!("{peer} is using {}", copy::THIS_COMPUTER_LOWER),
            format!("Someone at {peer} is typing and moving the pointer here."),
            platform::CAN_REFUSE_CONTROL.then(|| {
                let name = peer.clone();
                Button::new("refuse-control")
                    .small()
                    .label(format!("Don’t let {peer} in"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(
                            Request::SetPeer {
                                name: name.clone(),
                                allow_control: Some(false),
                                keyboard: None,
                                reverse_scroll: None,
                                share_screen: None,
                            },
                            cx,
                        );
                    }))
                    .into_any_element()
            }),
        ),
        _ if let Some(row) = problem => (
            if row.level == Level::Error {
                Tone::Danger
            } else {
                Tone::Warning
            },
            row.title.clone(),
            row.detail.clone(),
            Some(
                Button::new("panel-fix")
                    .small()
                    .primary()
                    .label("Open Nidus")
                    .on_click(cx.listener(go(Page::Computers)))
                    .into_any_element(),
            ),
        ),
        State::Ready => (
            Tone::Success,
            snapshot.status.title.clone(),
            board::neighbor(snapshot).map_or_else(
                || "Put a computer next to your screens.".to_owned(),
                |(peer, side)| {
                    format!(
                        "Push past the {} edge to reach {peer}.",
                        side.to_lowercase()
                    )
                },
            ),
            None,
        ),
        _ => (
            Tone::Neutral,
            snapshot.status.title.clone(),
            copy::status_line(&snapshot.status, None)
                .1
                .unwrap_or_default(),
            None,
        ),
    };
    let dot = match state {
        State::Controlling | State::Controlled => widgets::state_tone(state),
        _ if problem.is_some() => tone,
        _ => widgets::state_tone(state),
    };
    let notice = Notice::new(tone, title).dot(dot).stacked().detail(detail);
    match action {
        Some(action) => notice.action(action),
        None => notice,
    }
    .into_any_element()
}

fn computers(snapshot: &Snapshot, cx: &mut Context<Panel>) -> AnyElement {
    let found: Vec<_> = snapshot
        .unplaced
        .iter()
        .filter(|computer| copy::placeable(computer))
        .collect();
    v_flex()
        .child(
            Label::new(TextStyle::Caption, "Computers")
                .strong()
                .muted()
                .pb(Space::S2),
        )
        .children(snapshot.peers.iter().map(|peer| {
            let name = peer.name.clone();
            row(
                SharedString::from(format!("peer-{}", peer.name)),
                widgets::state_dot(Some(peer.state)),
                peer.name.clone(),
                peer_action(&peer.name, peer.state),
            )
            .on_click(cx.listener(go(Page::Computer(name))))
            .into_any_element()
        }))
        .children(found.into_iter().map(|computer| {
            row(
                SharedString::from(format!("found-{}", computer.id)),
                widgets::state_dot(Some(PeerState::Paired)),
                format!("{} · nearby", computer.name),
                Button::new(SharedString::from(format!("add-{}", computer.id)))
                    .small()
                    .label("Add…")
                    .on_click(cx.listener(go(Page::Computers)))
                    .into_any_element(),
            )
            .into_any_element()
        }))
        .when(snapshot.peers.is_empty(), |list| {
            list.child(
                h_flex()
                    .h(px(ROW))
                    .gap(px(10.0))
                    .child(
                        Label::new(TextStyle::Body, "No other computers yet.")
                            .muted()
                            .flex_1(),
                    )
                    .child(
                        Button::new("set-up")
                            .small()
                            .label("Set up…")
                            .on_click(cx.listener(go(Page::Computers))),
                    ),
            )
        })
        .into_any_element()
}

fn row(id: SharedString, dot: penna::Dot, label: String, action: AnyElement) -> ListItem {
    ListItem::new(id)
        .h(px(ROW))
        .px(Space::S4)
        .mx(px(-8.0))
        .child(
            h_flex()
                .gap(Space::S5)
                .child(dot)
                .child(
                    Label::new(TextStyle::Lead, label)
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                )
                .child(action),
        )
}

fn peer_action(name: &str, state: PeerState) -> AnyElement {
    #[cfg(target_os = "macos")]
    if copy::linked(state) && state != PeerState::ControlledFromHere {
        let name = name.to_owned();
        return Button::new(SharedString::from(format!("look-{name}")))
            .small()
            .icon(penna::IconName::Eye)
            .label("Look")
            .on_click(move |_, _, cx| crate::lens::look(&name, cx))
            .into_any_element();
    }
    let _ = name;
    Label::new(TextStyle::Small, copy::peer_state(state))
        .muted()
        .into_any_element()
}

fn footer(cx: &mut Context<Panel>) -> AnyElement {
    let buttons = h_flex()
        .mx(px(-12.0))
        .gap(px(4.0))
        .child(
            Button::new("panel-arrange")
                .ghost()
                .label("Arrange…")
                .on_click(cx.listener(go(Page::Computers))),
        )
        .child(
            Button::new("panel-settings")
                .ghost()
                .label("Settings…")
                .on_click(cx.listener(go(Page::Settings))),
        )
        .child(div().flex_1())
        .child(
            Button::new("panel-quit")
                .ghost()
                .label("Quit")
                .on_click(|_, _, cx| {
                    #[cfg(not(target_os = "linux"))]
                    platform::quit(cx);
                    #[cfg(target_os = "linux")]
                    cx.quit();
                }),
        );
    v_flex()
        .gap(Space::S3)
        .child(Separator::horizontal())
        .child(buttons)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use ala::{Bounds, point, px, size};
    use nid::app::api::State;

    use super::{Look, WIDTH, look, place};
    use crate::board::tests::snapshot;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<ala::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn the_panel_hangs_under_a_menu_bar_icon_and_stays_on_screen() {
        let screen = rect(0.0, 25.0, 1512.0, 920.0);
        let middle = place(rect(700.0, 0.0, 24.0, 24.0), screen, 400.0);
        assert_eq!(middle.origin, point(px(712.0 - WIDTH / 2.0), px(30.0)));
        let corner = place(rect(1480.0, 0.0, 24.0, 24.0), screen, 400.0);
        assert_eq!(f32::from(corner.right()), 1504.0);
    }

    #[test]
    fn the_panel_stands_above_a_taskbar_icon() {
        let screen = rect(1920.0, 0.0, 1920.0, 1040.0);
        let panel = place(rect(3700.0, 1048.0, 24.0, 24.0), screen, 400.0);
        assert_eq!(f32::from(panel.bottom()), 1042.0);
        assert!(f32::from(panel.left()) >= 1928.0);
    }

    #[test]
    fn the_icon_says_where_the_keyboard_is() {
        let mut shown = snapshot();
        assert_eq!(look(Some(&shown), false), Look::Ready);
        assert_eq!(look(Some(&shown), true), Look::Update);
        shown.status.state = State::Controlling;
        assert_eq!(look(Some(&shown), true), Look::Away);
        shown.status.state = State::Controlled;
        assert_eq!(look(Some(&shown), false), Look::Visited);
        shown.status.state = State::Paused;
        assert_eq!(look(Some(&shown), false), Look::Paused);
        assert_eq!(look(None, false), Look::Attention);
    }
}
