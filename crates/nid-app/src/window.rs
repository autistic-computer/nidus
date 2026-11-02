use ala::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use nid::app::api::{Request, State};
use penna::{
    ActiveTheme as _, Chip, Disableable as _, IconName, Label, Root, Selectable as _, TextStyle,
    Tone, WindowControls, WindowExt as _, h_flex,
    input::InputState,
    shell::{app_shell_surface, app_titlebar_strip, window_drag_handle},
    sidebar::chrome_button,
    switch::Switch,
};

use crate::{
    board::{self, AddByAddress, Board},
    copy,
    model::{Model, Page, Snapshot},
    pages::{self, first_run::Step},
    platform, widgets,
};

pub struct NidWindow {
    pub model: Entity<Model>,
    pub services: Entity<platform::Services>,
    pub board: Entity<Board>,
    pub address: Entity<InputState>,
    pub address_error: Option<String>,
    pub looking_up: bool,
    pub copied: bool,
    pub first_run: Option<Step>,
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
            cx.observe(&model, |this, model, cx| {
                let (state, added) = {
                    let model = model.read(cx);
                    (
                        model
                            .snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.status.state),
                        model.added_by.is_some(),
                    )
                };
                if let Some(state) = state {
                    pages::first_run::watch(&mut this.first_run, state);
                }
                if added && matches!(this.first_run, Some(Step::Place { .. } | Step::Try { .. })) {
                    model.update(cx, Model::welcomed);
                }
                cx.notify();
            }),
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
            first_run: None,
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
                Label::new(TextStyle::Body, "Settings")
                    .strong()
                    .into_any_element(),
            )
        } else if let Some(steps) = pages::first_run::strip(self, cx) {
            Some(steps)
        } else {
            snapshot.as_ref().map(status_pill)
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
                this.child(Label::new(TextStyle::Small, "Sharing").muted())
                    .child(
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

fn status_pill(snapshot: &Snapshot) -> AnyElement {
    let status = &snapshot.status;
    let neighbor = board::neighbor(snapshot);
    let (title, hint) = copy::status_line(
        status,
        neighbor.as_ref().map(|(peer, side)| (peer.as_str(), *side)),
    );
    let away = status.state == State::Controlling;
    let chip = Chip::new("status", title)
        .dot(widgets::state_tone(status.state))
        .tone(if away { Tone::Primary } else { Tone::Neutral })
        .selected(away);
    match hint {
        Some(hint) => chip.detail(hint),
        None => chip,
    }
    .into_any_element()
}

impl Focusable for NidWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NidWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = pages::page(self, window, cx);
        let titlebar = self.titlebar(window, cx);
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
