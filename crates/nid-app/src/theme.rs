use ala::{App, Window};
use penna::{
    ADAPTIVE_CORNER_FRACTION, CORNER_SMOOTHING, Highlight, Kind, Shadow, StylePatch, SurfaceRole,
    Theme, interface_theme,
};

const P: StylePatch = StylePatch::EMPTY;

pub const BOARD: SurfaceRole = SurfaceRole::new(
    "board",
    Kind::Card,
    "Arrangement",
    "The well the computers' screens are arranged on",
)
.with(P.shadow(Shadow::None));
pub const SCREEN: SurfaceRole = SurfaceRole::new(
    "board.screen",
    Kind::Card,
    "Screens",
    "A screen on the arrangement",
)
.with(P.shadow(Shadow::Contact));
pub const CHIP: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "board.chip",
    "Computer names",
    "The name and state above each computer on the arrangement",
)
.with(P.selected(Highlight::Tint));
pub const SHELF_TILE: SurfaceRole = SurfaceRole::new(
    "board.found",
    Kind::Card,
    "Found computers",
    "Computers found nearby, in the dock under the arrangement",
);
pub const BANNER: SurfaceRole = SurfaceRole::new(
    "banner",
    Kind::Card,
    "Banners",
    "Problems and notes above the page",
)
.with(P.shadow(Shadow::None));

pub const STATUS: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "status",
    "Status",
    "What Nidus is doing, in the middle of the title bar",
)
.with(P.selected(Highlight::Tint));
pub const CARD: SurfaceRole = SurfaceRole::under(
    &penna::roles::SIDEBAR,
    "computer-card",
    "Computer card",
    "The selected computer's settings, beside the arrangement",
);

pub const STEP: SurfaceRole = SurfaceRole::under(
    &SurfaceRole::BADGE,
    "first-run.step",
    "First-run steps",
    "The numbered steps in the title bar while setting up",
)
.with(P.selected(Highlight::Accent));

const ROLES: &[SurfaceRole] = &[BOARD, SCREEN, CHIP, SHELF_TILE, BANNER, STATUS, CARD, STEP];

pub fn init(cx: &mut App) {
    penna::roles::register(ROLES);
    penna::init(cx);
    apply(None, cx);
}

pub fn apply(mut window: Option<&mut Window>, cx: &mut App) {
    if let Some(window) = window.as_deref_mut() {
        window.set_default_corner_smoothing(CORNER_SMOOTHING);
        window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
    }
    Theme::sync_system_appearance(window, cx);
    interface_theme::default_theme().apply(Theme::global_mut(cx));
    cx.refresh_windows();
}
