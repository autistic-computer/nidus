use ala::Context;
use nid::app::api::Request;
use penna::{
    button::Button,
    settings::{SettingEntry, settings_card_ground},
    switch::Switch,
};

use crate::{model::Snapshot, window::NidWindow};

pub fn login_entry(
    _: &mut NidWindow,
    snapshot: &Snapshot,
    cx: &mut Context<NidWindow>,
) -> Option<SettingEntry> {
    let enabled = snapshot.autostart?;
    Some(
        SettingEntry::new("Start at login")
            .description("Keep Nidus available after you close this window.")
            .control(
                Switch::new("autostart")
                    .checked(enabled)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.send(Request::SetAutostart { enabled: *on }, cx);
                    })),
            ),
    )
}

pub fn this_computer_entries(
    _: &mut NidWindow,
    snapshot: &Snapshot,
    cx: &mut Context<NidWindow>,
) -> Vec<SettingEntry> {
    vec![
        SettingEntry::new("Service log")
            .description("Follow what the background service is doing.")
            .control(
                Button::new("open-logs")
                    .label("Open")
                    .on(settings_card_ground(cx))
                    .on_click(cx.listener(|this, _, _, cx| this.send(Request::OpenLogs, cx))),
            ),
        SettingEntry::new("Configuration").description(snapshot.config_path.display().to_string()),
    ]
}
