mod computers;
mod inspector;
mod settings;

use ala::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{Dialog, DialogButtonProps},
    input::Input,
    rems_from_px, v_flex,
};
use zflow::app::api::{Action, Request};

use crate::{
    model::{Model, Page},
    platform, widgets,
    window::ZflowWindow,
};

pub fn page(
    this: &mut ZflowWindow,
    window: &mut Window,
    cx: &mut Context<ZflowWindow>,
) -> AnyElement {
    let model = this.model.read(cx);
    let content = match (&model.snapshot, &model.error) {
        (Some(_), _) => match model.page.clone() {
            Page::Computers => computers::render(this, window, cx),
            Page::Settings => settings::render(this, window, cx),
            Page::Computer(name) => {
                let owner = format!("peer:{name}");
                this.board
                    .update(cx, |board, cx| board.select(Some(owner), cx));
                this.model
                    .update(cx, |model, cx| model.show(Page::Computers, cx));
                computers::render(this, window, cx)
            }
        },
        (None, Some(error)) => unreachable_state(error.clone(), cx),
        (None, None) => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(penna::spinner::Spinner::new())
            .into_any_element(),
    };
    v_flex()
        .size_full()
        .min_h_0()
        .children(platform::banner(this, window, cx))
        .child(div().flex_1().min_h_0().child(content))
        .into_any_element()
}

fn unreachable_state(error: String, cx: &mut Context<ZflowWindow>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .p(px(32.0))
        .child(
            div()
                .text_size(rems_from_px(20.0))
                .font_medium()
                .child("zflow could not start"),
        )
        .child(
            div()
                .max_w(px(420.0))
                .text_center()
                .text_size(rems_from_px(13.0))
                .text_color(cx.theme().foreground.muted())
                .child(error),
        )
        .child(
            Button::new("try-again")
                .primary()
                .label("Try again")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model.update(cx, Model::reconnect);
                })),
        )
        .into_any_element()
}

pub fn fix_button(
    id: &str,
    action: &Action,
    ground: penna::Ground,
    cx: &mut Context<ZflowWindow>,
) -> AnyElement {
    let command = action.command.clone();
    Button::new(ala::SharedString::from(format!("fix-{id}")))
        .on(ground)
        .label(action.label.clone())
        .on_click(cx.listener(move |this, _, window, cx| {
            if !platform::fix(this, &command, window, cx) {
                match serde_json::from_value::<Request>(serde_json::json!({ "command": command })) {
                    Ok(request) => this.send(request, cx),
                    Err(error) => {
                        log::warn!("zflow offered a fix this window cannot send: {error}");
                    }
                }
            }
        }))
        .into_any_element()
}

pub fn add_address(this: &Entity<ZflowWindow>, dialog: Dialog, cx: &mut App) -> Dialog {
    let state = this.read(cx);
    let input = state.address.clone();
    let error = state.address_error.clone();
    let looking = state.looking_up;
    let typed = input.read(cx).value().trim().to_owned();
    let this = this.clone();
    dialog
        .title("Add a computer by address")
        .width(px(440.0))
        .child(
            v_flex()
                .gap(px(10.0))
                .child(widgets::muted(
                    "For a computer zflow can’t find on this network, like one on Tailscale.",
                    12.0,
                    cx,
                ))
                .child(Input::new(&input))
                .map(|this| match &error {
                    Some(error) => this.child(
                        div()
                            .text_size(rems_from_px(12.0))
                            .text_color(cx.theme().danger)
                            .child(error.clone()),
                    ),
                    None => this.child(widgets::muted(
                        "It shows up in Found on this network. Drag it into place to add it.",
                        12.0,
                        cx,
                    )),
                }),
        )
        .button_props(
            DialogButtonProps::default()
                .ok_text("Look up")
                .ok_variant(penna::button::ButtonVariant::Paint(
                    penna::Tone::Primary,
                    penna::Emphasis::Solid,
                ))
                .show_cancel(true),
        )
        .on_ok(move |_, window, cx| {
            if typed.is_empty() || looking {
                return false;
            }
            look_up(&this, typed.clone(), window, cx);
            false
        })
}

fn look_up(this: &Entity<ZflowWindow>, address: String, window: &mut Window, cx: &mut App) {
    let answer = this.update(cx, |this, cx| {
        this.looking_up = true;
        this.address_error = None;
        cx.notify();
        this.model.read(cx).attempt(Request::AddAddress { address })
    });
    let this = this.clone();
    window
        .spawn(cx, async move |cx| {
            let result = answer
                .recv()
                .await
                .unwrap_or_else(|_| Err("zflow stopped".into()));
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.looking_up = false;
                    match result {
                        Ok(()) => {
                            use penna::WindowExt as _;
                            window.close_dialog(cx);
                        }
                        Err(error) => this.address_error = Some(error),
                    }
                    cx.notify();
                });
            });
        })
        .detach();
}

pub fn card_danger(
    id: impl Into<ala::ElementId>,
    label: impl Into<ala::SharedString>,
    cx: &App,
) -> Button {
    Button::new(id)
        .danger()
        .label(label)
        .on(penna::settings::settings_card_ground(cx))
}

pub fn heading(
    title: impl Into<ala::SharedString>,
    description: impl Into<ala::SharedString>,
    cx: &App,
) -> ala::Div {
    penna::settings::settings_page_heading(title, cx)
        .gap(px(4.0))
        .child(
            div()
                .text_size(rems_from_px(11.0))
                .text_color(cx.theme().foreground.muted())
                .child(description.into()),
        )
}
