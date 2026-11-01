use ala::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use nid::app::api::{Request, State};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, IconName, PaintState, Root, StyledExt as _,
    Surface, Tone, WindowControls, WindowExt as _, h_flex,
    input::InputState,
    rems_from_px,
    shell::{app_shell_surface, app_titlebar_strip, window_drag_handle},
    sidebar::{chrome_button, status_dot},
    switch::Switch,
};

use crate::{
    board::{self, AddByAddress, Board},
    copy,
    model::{Model, Page, Snapshot},
    pages, platform,
    theme::STATUS,
    widgets,
};

pub struct NidWindow {
    pub model: Entity<Model>,
    pub services: Entity<platform::Services>,
    pub board: Entity<Board>,
    pub address: Entity<InputState>,
    pub address_error: Option<String>,
    pub looking_up: bool,
    pub copied: bool,
    focus: FocusHandle,
    _observers: Vec<Subscription>,
}

impl NidWindow {
    pub fn new(
        model: Entity<Model>,
        services: Entity<platform::Services>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let board = cx.new(|cx| Board::new(model.clone(), cx));
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("100.64.0.7"));
        let observers = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.observe(&services, |_, _, cx| cx.notify()),
            cx.observe(&board, |_, _, cx| cx.notify()),
            cx.subscribe_in(&board, window, |this, _, _: &AddByAddress, window, cx| {
                this.open_add_address(window, cx);
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                crate::theme::apply(Some(window), cx);
            }),
            cx.observe_in(&model, window, |_, model, window, cx| {
                if let Some(failure) = model.update(cx, |model, _| model.take_failure()) {
                    window.push_notification(
                        penna::notification::Notification::warning(failure)
                            .title("That didn’t work"),
                        cx,
                    );
                }
            }),
        ];
        Self {
            model,
            services,
            board,
            address,
            address_error: None,
            looking_up: false,
            copied: false,
            focus: cx.focus_handle(),
            _observers: observers,
        }
    }

    pub fn send(&self, request: Request, cx: &App) {
        self.model.read(cx).send(request);
    }

    pub fn open_add_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address_error = None;
        self.address
            .update(cx, |input, cx| input.set_value("", window, cx));
        let this = cx.entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            pages::add_address(&this, dialog, cx)
        });
    }

    fn titlebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let model = self.model.read(cx);
        let settings = model.page == Page::Settings;
        let snapshot = model.snapshot.clone();
        let sharing = snapshot
            .as_ref()
            .filter(|snapshot| !snapshot.peers.is_empty())
            .map(|snapshot| snapshot.sharing);
        let middle = if settings {
            Some(
                div()
                    .text_size(rems_from_px(13.0))
                    .font_medium()
                    .child("Settings")
                    .into_any_element(),
            )
        } else {
            snapshot.as_ref().map(|snapshot| status_pill(snapshot, cx))
        };
        let leading = h_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .gap(px(10.0))
            .pl(if cfg!(target_os = "macos") {
                penna::title_bar::macos_traffic_light_clearance(cx)
            } else {
                px(12.0)
            })
            .pr(px(8.0))
            .child(div().flex_1())
            .children(middle)
            .child(div().flex_1())
            .when_some(sharing, |this, sharing| {
                this.child(widgets::muted("Sharing", 12.0, cx)).child(
                    Switch::new("titlebar-sharing")
                        .checked(sharing == Some(true))
                        .disabled(sharing.is_none())
                        .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                            this.send(Request::SetSharing { enabled: *enabled }, cx);
                        })),
                )
            })
            .child(
                chrome_button(
                    "settings",
                    if settings {
                        IconName::Xmark
                    } else {
                        IconName::Settings
                    },
                    if settings {
                        "Back to your computers"
                    } else {
                        "Settings"
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let page = if settings {
                        Page::Computers
                    } else {
                        Page::Settings
                    };
                    this.model.update(cx, |model, cx| model.show(page, cx));
                })),
            );
        let controls = WindowControls::new().on_close_window(|_, window, cx| {
            if platform::may_close(cx) {
                window.remove_window();
            }
        });
        window_drag_handle(
            "titlebar-drag",
            app_titlebar_strip("titlebar", leading, controls, cx),
            window,
            cx,
        )
        .into_any_element()
    }
}

fn status_pill(snapshot: &Snapshot, cx: &App) -> AnyElement {
    let status = &snapshot.status;
    let neighbor = board::neighbor(snapshot);
    let (title, hint) = copy::status_line(
        status,
        neighbor.as_ref().map(|(peer, side)| (peer.as_str(), *side)),
    );
    let away = status.state == State::Controlling;
    h_flex()
        .min_w_0()
        .h(px(26.0))
        .px(px(12.0))
        .gap(px(6.0))
        .rounded_full()
        .surface_at(
            Surface::new(STATUS).tone(if away { Tone::Primary } else { Tone::Neutral }),
            if away {
                PaintState::Selected
            } else {
                PaintState::Rest
            },
            cx,
        )
        .child(status_dot(widgets::status_color(status.state, cx)))
        .child(
            div()
                .flex_none()
                .text_size(rems_from_px(12.0))
                .font_medium()
                .whitespace_nowrap()
                .child(title),
        )
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(rems_from_px(12.0))
                    .text_color(cx.theme().foreground.muted())
                    .child(format!("· {hint}")),
            )
        })
        .into_any_element()
}

impl Focusable for NidWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NidWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let titlebar = self.titlebar(window, cx);
        let content = pages::page(self, window, cx);
        let dialogs = Root::render_dialog_layer(window, cx).map(IntoElement::into_any_element);
        let notifications =
            Root::render_notification_layer(window, cx).map(IntoElement::into_any_element);
        app_shell_surface(
            SharedString::from("Nidus"),
            cx.theme().background,
            div(),
            Some(titlebar),
            content,
            dialogs.into_iter().chain(notifications),
        )
        .track_focus(&self.focus)
    }
}
