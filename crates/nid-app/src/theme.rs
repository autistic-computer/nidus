use ala::{App, Window};
use penna::{ADAPTIVE_CORNER_FRACTION, Theme, interface_theme};

pub fn init(cx: &mut App) {
    penna::init(cx);
    apply(None, cx);
}

pub fn apply(mut window: Option<&mut Window>, cx: &mut App) {
    if let Some(window) = window.as_deref_mut() {
        window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
    }
    Theme::sync_system_appearance(window, cx);
    interface_theme::default_theme().apply(Theme::global_mut(cx));
    cx.refresh_windows();
}
