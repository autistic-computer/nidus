use ala::{AnyElement, App, ParentElement as _, SharedString, Styled as _, div, px};
use nid::app::api::{Level, PeerState, State};
use penna::{Dot, Ground, Notice, Tone, h_flex};

pub fn banner(
    level: Level,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
) -> Notice {
    let notice = Notice::new(tone_of(level), title).detail(detail);
    match action {
        Some(action) => notice.action(action),
        None => notice,
    }
}

const fn tone_of(level: Level) -> Tone {
    match level {
        Level::Error => Tone::Danger,
        Level::Warning => Tone::Warning,
        Level::Ok => Tone::Success,
    }
}

pub fn banner_ground(level: Level, cx: &App) -> Ground {
    Notice::ground(tone_of(level), cx)
}

pub fn note(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Notice {
    Notice::new(Tone::Primary, title).detail(detail)
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
pub fn key_mark_sized(mark: &str, size: f32) -> ala::Div {
    h_flex()
        .gap(px(size / 3.5))
        .children(mark_indices(mark).into_iter().map(|index| {
            div()
                .size(px(size))
                .rounded(px(size / 3.5))
                .bg(ala::rgb(MARK_COLORS[index]))
        }))
}

pub fn key_mark(mark: &str) -> ala::Div {
    key_mark_sized(mark, 7.0)
}

pub const fn peer_tone(state: Option<PeerState>) -> Tone {
    match state {
        None | Some(PeerState::Connected) => Tone::Success,
        Some(PeerState::ControllingThis | PeerState::ControlledFromHere) => Tone::Accent,
        Some(PeerState::Connecting) => Tone::Warning,
        Some(PeerState::Paired | PeerState::Unreachable) => Tone::Neutral,
    }
}

pub fn state_dot(state: Option<PeerState>) -> Dot {
    Dot::new(peer_tone(state))
}

pub const fn state_tone(state: State) -> Tone {
    match state {
        State::Ready => Tone::Success,
        State::Controlling | State::Controlled => Tone::Accent,
        State::Checking => Tone::Warning,
        State::Attention => Tone::Danger,
        State::Paused | State::Setup => Tone::Neutral,
    }
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
