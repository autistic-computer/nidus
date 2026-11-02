use ala::{AnyElement, App, AppContext as _, Context, Entity, Window};
use nid::app::api::{Action, Health, Peer};
use penna::settings::{SettingEntry, SettingsStack};

use crate::{model::Snapshot, window::NidWindow};

pub const CAN_SHARE_SCREEN: bool = true;
pub const CAN_REFUSE_CONTROL: bool = true;
pub const OWN_SCREENS_NOTE: &str =
    "Between this computer’s own screens, the pointer follows the system’s display settings.";

#[derive(Default)]
pub struct Services;

pub fn init(cx: &mut App) -> Entity<Services> {
    let services = cx.new(|_| Services);
    cx.set_global(super::SharedServices(services.clone()));
    services
}

pub fn window_shown(_: &mut App) {}

pub fn may_close(_: &App) -> bool {
    true
}

pub fn notification_response(_: &ala::SystemNotificationResponse, _: &mut App) {}

pub fn quit(cx: &mut App) {
    cx.quit();
}

pub fn banner(_: &mut NidWindow, _: &mut Window, _: &mut Context<NidWindow>) -> Option<AnyElement> {
    None
}

pub fn fix(_: &mut NidWindow, _: &str, _: &mut Window, _: &mut Context<NidWindow>) -> bool {
    false
}

pub fn health_action(row: &Health, _: &App) -> Option<Action> {
    row.action.clone()
}

pub fn first_run_cards(
    _: &mut NidWindow,
    _: &Snapshot,
    _: &mut Window,
    _: &mut Context<NidWindow>,
) -> Vec<AnyElement> {
    Vec::new()
}

pub fn display_settings_entry(_: &App) -> Option<SettingEntry> {
    None
}

pub fn peer_actions(_: &mut NidWindow, _: &Peer, _: &mut Context<NidWindow>) -> Vec<AnyElement> {
    Vec::new()
}

pub fn settings_stacks(
    _: &mut NidWindow,
    _: &Snapshot,
    _: &mut Window,
    _: &mut Context<NidWindow>,
) -> Vec<SettingsStack> {
    Vec::new()
}
