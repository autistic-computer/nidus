use ala::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};
use nid::{
    app::{
        api::{Peer, Request},
        layout_model::Seam,
    },
    core::KeyboardMode,
    neighbors::Unplaced,
};
use penna::{
    Icon, IconName, Inspector, Label, Sizable as _, TextStyle, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{AlertDialog, DialogButtonProps},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    settings::{SettingEntry, SettingsStack, settings_card_ground},
    switch::Switch,
};

use crate::{
    board::{self, Board, Computer, FOUND, LOCAL, geometry::Side},
    copy,
    model::{Model, Snapshot},
    platform, widgets,
    window::NidWindow,
};

pub fn render(
    this: &mut NidWindow,
    snapshot: &Snapshot,
    window: &mut Window,
    cx: &mut Context<NidWindow>,
) -> Option<AnyElement> {
    let selection = this.board.read(cx).selection.clone()?;
    let card = if let Some(id) = selection.strip_prefix(FOUND) {
        let computer = snapshot
            .unplaced
            .iter()
            .find(|computer| computer.id == id)?;
        found_card(&this.board, computer)
    } else {
        let computers = board::computers(snapshot);
        let computer = computers
            .iter()
            .find(|computer| computer.owner == selection)?;
        computer_card(this, snapshot, computer, window, cx)
    };
    Some(card.into_any_element())
}

fn inspector(
    board: &Entity<Board>,
    name: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    mark: Option<&str>,
) -> Inspector {
    let board = board.clone();
    let inspector = Inspector::new("card", name)
        .subtitle(detail)
        .on_close(move |_, _, cx| {
            board.update(cx, |board, cx| board.select(None, cx));
        });
    match mark {
        Some(mark) => inspector.trailing(div().pt(px(8.0)).child(widgets::key_mark(mark))),
        None => inspector,
    }
}

fn summary(computer: &Computer) -> String {
    let screens = match computer.screens.len() {
        0 => "Not on the desk".to_owned(),
        1 => "1 screen".to_owned(),
        count => format!("{count} screens"),
    };
    match &computer.peer {
        Some(peer) => format!("{} · {screens}", copy::peer_state(peer.state)),
        None => format!("{} · {screens}", copy::THIS_COMPUTER),
    }
}

fn found_card(board: &Entity<Board>, computer: &Unplaced) -> Inspector {
    let card = inspector(
        board,
        computer.name.clone(),
        copy::unplaced_detail(computer),
        computer.mark.as_deref(),
    );
    let card = if let Some(note) = copy::unplaceable_note(computer) {
        card.child(Label::new(TextStyle::Body, note).muted())
    } else {
        let sides = Side::ALL.map(|side| {
            let (board, id) = (board.clone(), computer.id.clone());
            Button::new(SharedString::from(format!("side-{}", side.label())))
                .label(side.label())
                .on_click(move |_, _, cx| board.read(cx).place(&id, side, cx))
        });
        card.child(Label::new(TextStyle::Body, "Put it on this side of your screens:").muted())
            .child(h_flex().flex_wrap().gap(px(8.0)).children(sides))
            .child(Label::new(TextStyle::Small, "Or drag it onto the desk where it sits.").muted())
    };
    card.when(computer.mark.is_some(), |this| {
        this.child(
            Label::new(
                TextStyle::Small,
                format!(
                    "{} shows the same mark, so you can tell two with the same name apart.",
                    computer.name
                ),
            )
            .muted(),
        )
    })
}

fn computer_card(
    this: &mut NidWindow,
    snapshot: &Snapshot,
    computer: &Computer,
    window: &mut Window,
    cx: &mut Context<NidWindow>,
) -> Inspector {
    let board = this.board.clone();
    let adjusting = board.read(cx).adjusting.as_deref() == Some(computer.owner.as_str());
    let local = computer.owner == LOCAL;

    let mut arrange = Vec::new();
    if computer.screens.len() > 1 {
        let owner = computer.owner.clone();
        let adjust = board.clone();
        arrange.push(
            SettingEntry::new("Move screens one by one")
                .description("Double-clicking it on the desk does too.")
                .control(Switch::new("adjust").checked(adjusting).on_click(
                    move |on: &bool, _, cx| {
                        adjust.update(cx, |board, cx| board.adjust(&owner, *on, cx));
                    },
                )),
        );
    }
    if local {
        arrange.extend(platform::display_settings_entry(cx));
    }

    let mine: Vec<&str> = computer
        .screens
        .iter()
        .map(|monitor| monitor.id.as_str())
        .collect();
    let crossings = board::crossings(snapshot, computer);
    let crossing_entries: Vec<SettingEntry> = if crossings.is_empty() {
        vec![
            SettingEntry::new("None yet")
                .description("Move it until an edge touches another computer’s screen."),
        ]
    } else {
        crossings
            .iter()
            .map(|seam| crossing_entry(snapshot, seam, &mine))
            .collect()
    };
    let mut crossings_stack = SettingsStack::titled("Crossings").children(crossing_entries);
    if local && computer.screens.len() > 1 {
        crossings_stack = crossings_stack.note(platform::OWN_SCREENS_NOTE);
    }
    let screens =
        SettingsStack::titled("Screens").children(computer.screens.iter().map(|monitor| {
            let detail = monitor
                .display
                .as_ref()
                .map_or_else(String::new, |display| {
                    copy::screen_size(
                        display.width_mm,
                        display.height_mm,
                        display.bounds.width,
                        display.bounds.height,
                    )
                });
            SettingEntry::new(
                monitor
                    .display
                    .as_ref()
                    .map_or_else(|| monitor.label.clone(), |display| display.name.clone()),
            )
            .description(detail)
        }));

    inspector(
        &board,
        computer.name.clone(),
        summary(computer),
        computer.mark.as_deref(),
    )
    .when_some(computer.peer.as_ref(), |column, peer| {
        let actions = platform::peer_actions(this, peer, cx);
        column
            .when(!actions.is_empty(), |column| {
                column.child(h_flex().flex_wrap().gap(px(8.0)).children(actions))
            })
            .children(peer_settings(this, peer, window, cx))
    })
    .when(!arrange.is_empty(), |this| {
        this.child(SettingsStack::new().children(arrange))
    })
    .when(!computer.screens.is_empty(), |this| {
        this.child(crossings_stack).child(screens)
    })
    .when_some(computer.peer.as_ref(), |column, peer| {
        column.child(forget_button(&this.model, peer))
    })
}

fn crossing_entry(snapshot: &Snapshot, seam: &Seam, mine: &[&str]) -> SettingEntry {
    let here_is_a = mine.contains(&seam.a.as_str());
    let (here, there) = if here_is_a {
        (&seam.a, &seam.b)
    } else {
        (&seam.b, &seam.a)
    };
    let screen = |id: &str| {
        board::monitors(snapshot)
            .find(|monitor| monitor.id == id)
            .map_or_else(
                || "a screen".to_owned(),
                |monitor| {
                    monitor
                        .display
                        .as_ref()
                        .map_or_else(|| monitor.label.clone(), |display| display.name.clone())
                },
            )
    };
    let other = board::monitors(snapshot)
        .find(|monitor| &monitor.id == there)
        .map_or_else(
            || there.clone(),
            |monitor| {
                monitor
                    .peer
                    .clone()
                    .unwrap_or_else(|| copy::THIS_COMPUTER.to_owned())
            },
        );
    SettingEntry::new(other)
        .description(format!(
            "{} edge of {} → {}",
            board::geometry::crossing_side(seam, here_is_a),
            screen(here),
            screen(there)
        ))
        .title_icon(Icon::new(if seam.vertical {
            IconName::ArrowRight
        } else {
            IconName::ArrowDown
        }))
}

fn peer_settings(
    this: &mut NidWindow,
    peer: &Peer,
    window: &mut Window,
    cx: &mut Context<NidWindow>,
) -> Vec<AnyElement> {
    let _ = window;
    let name = peer.name.clone();
    let set = |name: &str| {
        let name = name.to_owned();
        move |allow_control: Option<bool>,
              keyboard: Option<KeyboardMode>,
              reverse_scroll: Option<bool>,
              share_screen: Option<bool>| Request::SetPeer {
            name: name.clone(),
            allow_control,
            keyboard,
            reverse_scroll,
            share_screen,
        }
    };
    let model = this.model.clone();

    let mut allowed = Vec::new();
    if platform::CAN_REFUSE_CONTROL {
        let (model, set) = (model.clone(), set(&name));
        allowed.push(
            SettingEntry::new(format!("Control {}", copy::THIS_COMPUTER_LOWER))
                .description(format!(
                    "Push past {name}’s edge to come over. {} takes it back.",
                    copy::RETURN_KEYS
                ))
                .control(
                    Switch::new("allow-control")
                        .checked(peer.allow_control)
                        .on_click(move |on: &bool, _, cx| {
                            model.read(cx).send(set(Some(*on), None, None, None));
                        }),
                ),
        );
    }
    if platform::CAN_SHARE_SCREEN {
        let (model, set) = (model.clone(), set(&name));
        allowed.push(
            SettingEntry::new("See this screen")
                .description("Through a lens around its pointer.")
                .control(
                    Switch::new("share-screen")
                        .checked(peer.share_screen)
                        .on_click(move |on: &bool, _, cx| {
                            model.read(cx).send(set(None, None, None, Some(*on)));
                        }),
                ),
        );
    }

    let keyboard = {
        let (model, set) = (model.clone(), set(&name));
        let current = peer.keyboard;
        SettingEntry::new("Keys")
            .description(copy::keyboard_detail(current))
            .control(
                Button::new("keyboard-mode")
                    .small()
                    .on(settings_card_ground(cx))
                    .label(copy::keyboard_title(current))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        copy::KEYBOARD_MODES.iter().fold(menu, |menu, &mode| {
                            let (model, set) = (model.clone(), set.clone());
                            menu.item(
                                PopupMenuItem::new(copy::keyboard_title(mode))
                                    .checked(mode == current)
                                    .on_click(move |_, _, cx| {
                                        model.read(cx).send(set(None, Some(mode), None, None));
                                    }),
                            )
                        })
                    }),
            )
    };
    let scroll = {
        let (model, set) = (model.clone(), set(&name));
        SettingEntry::new("Reverse scrolling")
            .description("Turn its scrolling around here.")
            .control(
                Switch::new("reverse-scroll")
                    .checked(peer.reverse_scroll)
                    .on_click(move |on: &bool, _, cx| {
                        model.read(cx).send(set(None, None, Some(*on), None));
                    }),
            )
    };
    let typing = [keyboard, scroll];

    let mut stacks = Vec::new();
    if !allowed.is_empty() {
        stacks.push(
            SettingsStack::titled(format!("What {name} can do here"))
                .children(allowed)
                .into_any_element(),
        );
    }
    stacks.push(
        SettingsStack::titled(format!("When {name} types here"))
            .children(typing)
            .into_any_element(),
    );
    stacks
}

fn forget_button(model: &Entity<Model>, peer: &Peer) -> AnyElement {
    let (name, model) = (peer.name.clone(), model.clone());
    h_flex()
        .child(
            Button::new("forget")
                .danger()
                .ghost()
                .label(format!("Forget {name}…"))
                .on_click(move |_, window, cx| {
                    let (name, model) = (name.clone(), model.clone());
                    window.open_alert_dialog(cx, move |alert, _, _| {
                        forget_alert(alert, &name, &model)
                    });
                }),
        )
        .into_any_element()
}

pub fn forget_alert(alert: AlertDialog, name: &str, model: &Entity<Model>) -> AlertDialog {
    let (name, model) = (name.to_owned(), model.clone());
    alert
        .title(format!("Forget {name}?"))
        .description("This stops input between the two computers and removes its trusted identity. It goes back to Nearby.")
        .button_props(
            DialogButtonProps::default()
                .ok_text("Forget")
                .ok_variant(penna::button::ButtonVariant::Paint(
                    penna::Tone::Danger,
                    penna::Emphasis::Solid,
                ))
                .show_cancel(true),
        )
        .on_ok(move |_, _, cx| {
            model.read(cx).send(Request::Forget { name: name.clone() });
            true
        })
}
