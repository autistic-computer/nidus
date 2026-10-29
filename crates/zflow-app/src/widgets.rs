use ala::{
    AnyElement, App, Hsla, ParentElement as _, SharedString, Styled as _, div,
    prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Ground, Icon, IconName, Sizable as _, StyledExt as _, Surface,
    Tone, h_flex, rems_from_px, sidebar::status_dot, v_flex,
};
use zflow::app::api::{Level, PeerState, State};

use crate::theme::BANNER;

pub fn banner(
    level: Level,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> ala::Div {
    let tone = tone_of(level);
    let icon = match level {
        Level::Error => IconName::CircleX,
        Level::Warning => IconName::TriangleAlert,
        Level::Ok => IconName::CircleCheck,
    };
    callout(tone, icon, title, detail, action, cx)
}

const fn tone_of(level: Level) -> Tone {
    match level {
        Level::Error => Tone::Danger,
        Level::Warning => Tone::Warning,
        Level::Ok => Tone::Success,
    }
}

pub fn banner_ground(level: Level, cx: &App) -> Ground {
    cx.theme()
        .ground_of(Surface::new(BANNER).tone(tone_of(level)))
}

pub fn note(title: impl Into<SharedString>, detail: impl Into<SharedString>, cx: &App) -> ala::Div {
    callout(Tone::Primary, IconName::Info, title, detail, None, cx)
}

fn callout(
    tone: Tone,
    icon: IconName,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> ala::Div {
    let detail = detail.into();
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(10.0))
        .surface(Surface::new(BANNER).tone(tone), cx)
        .child(
            div().pt(px(1.0)).child(
                Icon::new(icon)
                    .with_size(px(16.0))
                    .text_color(cx.theme().tone(tone)),
            ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(rems_from_px(13.0))
                        .font_medium()
                        .child(title.into()),
                )
                .when(!detail.is_empty(), |this| {
                    this.child(
                        div()
                            .text_size(rems_from_px(12.0))
                            .text_color(cx.theme().foreground.muted())
                            .child(detail),
                    )
                }),
        )
        .children(action)
}

const MARK_COLORS: [u32; 8] = [
    0x00E5_484D,
    0x00F7_6B15,
    0x00FF_C53D,
    0x0030_A46C,
    0x0012_A594,
    0x0000_90FF,
    0x008E_4EC6,
    0x00D6_409F,
];

pub fn mark_indices(mark: &str) -> Vec<usize> {
    if mark.len() != 6 || !mark.chars().all(|c| c.is_ascii_hexdigit()) {
        return Vec::new();
    }
    mark.chars()
        .take(4)
        .filter_map(|digit| digit.to_digit(16))
        .map(|digit| (digit & 7) as usize)
        .collect()
}

#[allow(
    clippy::disallowed_methods,
    reason = "a key's mark is an identity, drawn in the same colors on every computer and in every theme"
)]
pub fn key_mark(mark: &str) -> ala::Div {
    h_flex()
        .gap(px(2.0))
        .children(mark_indices(mark).into_iter().map(|index| {
            div()
                .size(px(7.0))
                .rounded(px(2.0))
                .bg(ala::rgb(MARK_COLORS[index]))
        }))
}

pub fn state_dot(state: Option<PeerState>, cx: &App) -> ala::Div {
    let theme = cx.theme();
    status_dot(match state {
        None | Some(PeerState::Connected) => theme.success,
        Some(PeerState::ControllingThis | PeerState::ControlledFromHere) => theme.accent,
        Some(PeerState::Connecting) => theme.warning,
        Some(PeerState::Paired | PeerState::Unreachable) => theme.foreground.faint(),
    })
}

pub fn status_color(state: State, cx: &App) -> Hsla {
    let theme = cx.theme();
    match state {
        State::Ready => theme.success,
        State::Controlling | State::Controlled => theme.accent,
        State::Checking => theme.warning,
        State::Attention => theme.danger,
        State::Paused | State::Setup => theme.foreground.muted(),
    }
}

pub fn muted(text: impl Into<SharedString>, size: f32, cx: &App) -> ala::Div {
    div()
        .text_size(rems_from_px(size))
        .text_color(cx.theme().foreground.muted())
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_pick_their_colors_from_the_first_four_digits() {
        assert_eq!(mark_indices("f04ee5"), vec![7, 0, 4, 6]);
        assert_eq!(mark_indices("996615"), vec![1, 1, 6, 6]);
        assert!(mark_indices("xyz").is_empty());
        assert!(mark_indices("").is_empty());
    }
}
