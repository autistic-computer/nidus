//! What applies to this computer as a whole. Each paired computer's own
//! settings are in its inspector on the Computers page.

use zflow::app::api::{Level, Request};
use zz_gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use zz_gpui_kit::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    kbd::Keycap,
    rems_from_px,
    settings::{SettingEntry, SettingsStack, settings_scroll_column},
    switch::Switch,
};

use crate::{platform, window::ZflowWindow};

pub fn render(
    this: &mut ZflowWindow,
    window: &mut Window,
    cx: &mut Context<ZflowWindow>,
) -> AnyElement {
    let Some(snapshot) = this.model.read(cx).snapshot.clone() else {
        return div().into_any_element();
    };
    let mut general = Vec::new();
    general.extend(platform::login_entry(this, &snapshot, cx));
    if let Some(share) = snapshot.share_clipboard {
        general.push(
            SettingEntry::new(
                "Share clipboard",
                "Text and images go with the pointer to the other computer. Never files.",
            )
            .control(Switch::new("share-clipboard").checked(share).on_click(
                cx.listener(|this, on: &bool, _, cx| {
                    this.send(Request::SetClipboard { share: *on }, cx);
                }),
            )),
        );
    }
    if let Some(pause) = snapshot.pause_at_edges {
        general.push(
            SettingEntry::new(
                "Pause at edges",
                "Rest against an edge for a moment before switching.",
            )
            .control(Switch::new("pause-at-edges").checked(pause).on_click(
                cx.listener(|this, on: &bool, _, cx| {
                    this.send(
                        Request::SetSwitching {
                            pause_at_edges: *on,
                        },
                        cx,
                    );
                }),
            )),
        );
    }

    let shortcuts = snapshot.shortcuts.iter().map(|shortcut| {
        SettingEntry::new(shortcut.title.clone(), "").control(
            div().flex().gap(px(4.0)).children(
                shortcut
                    .keys
                    .split('+')
                    .map(|key| Keycap::new(key.trim().to_owned())),
            ),
        )
    });

    let mut column = settings_scroll_column("settings")
        .child(super::heading(
            "Settings",
            "What applies to this computer as a whole.",
            cx,
        ))
        .child(SettingsStack::titled("General").children(general));
    for stack in platform::settings_stacks(this, &snapshot, window, cx) {
        column = column.child(stack);
    }
    if !snapshot.shortcuts.is_empty() {
        column = column.child(SettingsStack::titled("Shortcuts").children(shortcuts));
    }
    // Rows that are fine stay listed, so people see what was checked.
    if !snapshot.health.is_empty() {
        let theme = cx.theme().clone();
        let checks: Vec<SettingEntry> = snapshot
            .health
            .iter()
            .map(|row| {
                let (icon, color) = match row.level {
                    Level::Ok => (IconName::CircleCheck, theme.success),
                    Level::Warning => (IconName::TriangleAlert, theme.warning),
                    Level::Error => (IconName::CircleX, theme.danger),
                };
                let entry = SettingEntry::new(row.title.clone(), row.detail.clone())
                    .title_icon(Icon::new(icon).text_color(color));
                match platform::health_action(row, cx) {
                    Some(action) => entry.control(super::fix_button(&row.id, &action, cx)),
                    None => entry,
                }
            })
            .collect();
        column = column.child(SettingsStack::titled("Checks").children(checks));
    }
    column
        .child(
            SettingsStack::titled("This computer")
                .children(platform::this_computer_entries(this, &snapshot, cx)),
        )
        .child(
            div()
                .text_size(rems_from_px(11.0))
                .text_color(cx.theme().foreground.muted())
                .child(format!(
                    "Changes save automatically. Advanced settings are in {}.",
                    snapshot.config_path.display()
                )),
        )
        .into_any_element()
}
