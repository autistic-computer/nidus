use ala::{
    AnyElement, App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    div, prelude::FluentBuilder as _, px,
};
use nid::{
    app::api::{Peer, Request, State},
    hello::Notice,
    neighbors::Unplaced,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, PaintState,
    Sizable as _, StyledExt, Surface, Tone,
    button::{Button, ButtonVariant, ButtonVariants},
    check::Check,
    h_flex,
    kbd::Keycap,
    rems_from_px,
    segmented::SegmentedControl,
    spinner::Spinner,
    v_flex,
};

use super::computers::{host_name, problems};
use crate::{
    board::{self, geometry::Side},
    copy,
    model::{Model, Snapshot},
    platform,
    theme::{SHELF_TILE, STEP},
    widgets,
    window::NidWindow,
};

const STEPS: &[&str] = if cfg!(target_os = "macos") {
    &["Allow", "Find", "Place"]
} else {
    &["Find", "Place"]
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Allow,
    Find,
    Place {
        id: String,
        name: String,
        mark: Option<String>,
    },
    Try {
        name: String,
        reached: bool,
        back: bool,
    },
}

pub fn next(step: Option<Step>, snapshot: &Snapshot, asks: bool) -> Option<Step> {
    match step {
        Some(step @ (Step::Place { .. } | Step::Try { .. })) => Some(step),
        _ if !snapshot.peers.is_empty() => None,
        Some(Step::Find) => Some(Step::Find),
        Some(Step::Allow) | None => Some(if asks { Step::Allow } else { Step::Find }),
    }
}

pub fn watch(step: &mut Option<Step>, state: State) {
    if let Some(Step::Try { reached, back, .. }) = step {
        match state {
            State::Controlling => *reached = true,
            State::Ready if *reached => *back = true,
            _ => {}
        }
    }
}

pub fn render(
    this: &mut NidWindow,
    snapshot: &Snapshot,
    window: &mut Window,
    cx: &mut Context<NidWindow>,
) -> Option<AnyElement> {
    let cards = platform::first_run_cards(this, snapshot, window, cx);
    this.first_run = next(this.first_run.take(), snapshot, !cards.is_empty());
    let step = this.first_run.clone()?;
    this.board
        .update(cx, |board, cx| board.set_docked(false, cx));
    Some(match step {
        Step::Allow => allow(cards, cx),
        Step::Find => find(this, snapshot, cx),
        Step::Place { id, name, mark } => place(this, snapshot, &id, &name, mark.as_deref(), cx),
        Step::Try {
            name,
            reached,
            back,
        } => try_it(this, snapshot, &name, reached, back, cx),
    })
}

pub fn strip(this: &NidWindow, cx: &App) -> Option<AnyElement> {
    let now = if this.model.read(cx).added_by.is_some() {
        STEPS.len()
    } else {
        match this.first_run.as_ref()? {
            Step::Allow => 0,
            Step::Find => STEPS.len() - 2,
            Step::Place { .. } => STEPS.len() - 1,
            Step::Try { .. } => return None,
        }
    };
    Some(
        h_flex()
            .gap(px(10.0))
            .children(STEPS.iter().enumerate().map(|(index, label)| {
                let (tone, state) = match index.cmp(&now) {
                    std::cmp::Ordering::Less => (Tone::Primary, PaintState::Rest),
                    std::cmp::Ordering::Equal => (Tone::Primary, PaintState::Selected),
                    std::cmp::Ordering::Greater => (Tone::Neutral, PaintState::Rest),
                };
                h_flex()
                    .gap(px(10.0))
                    .when(index > 0, |this| {
                        this.child(
                            div()
                                .w(px(28.0))
                                .h(px(2.0))
                                .surface(Surface::new(STEP).tone(tone), cx),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(20.0))
                            .rounded_full()
                            .text_size(rems_from_px(11.0))
                            .font_medium()
                            .surface_at(Surface::new(STEP).tone(tone), state, cx)
                            .map(|dot| {
                                if index < now {
                                    dot.child(Icon::new(IconName::Check).with_size(px(12.0)))
                                } else {
                                    dot.child((index + 1).to_string())
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(rems_from_px(13.0))
                            .when(index == now, StyledExt::font_medium)
                            .when(index > now, |this| {
                                this.text_color(cx.theme().foreground.muted())
                            })
                            .child(*label),
                    )
            }))
            .into_any_element(),
    )
}

fn frame(width: f32, body: ala::Div) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .px(px(32.0))
        .pt(px(28.0))
        .pb(px(24.0))
        .child(
            body.w_full()
                .max_w(px(width))
                .flex_1()
                .min_h_0()
                .gap(px(8.0)),
        )
        .into_any_element()
}

fn title(text: impl Into<SharedString>) -> ala::Div {
    div()
        .text_size(rems_from_px(26.0))
        .font_medium()
        .child(text.into())
}

fn footer(
    left: impl IntoElement,
    hint: impl Into<SharedString>,
    right: impl IntoElement,
    cx: &App,
) -> ala::Div {
    h_flex()
        .w_full()
        .gap(px(10.0))
        .child(left)
        .child(widgets::muted(hint, 13.0, cx).flex_1().text_center())
        .child(right)
}

fn allow(cards: Vec<AnyElement>, cx: &mut Context<NidWindow>) -> AnyElement {
    frame(
        520.0,
        v_flex()
            .child(title(format!(
                "Let Nidus work on {}",
                copy::THIS_COMPUTER_LOWER
            )))
            .child(widgets::muted(
                "macOS asks once for each. You can change them later in System Settings.",
                14.0,
                cx,
            ))
            .child(v_flex().pt(px(14.0)).gap(px(10.0)).children(cards))
            .child(div().flex_1())
            .child(
                h_flex().justify_end().child(
                    Button::new("allow-continue")
                        .label("Continue")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.first_run = Some(Step::Find);
                            cx.notify();
                        })),
                ),
            ),
    )
}

fn find(this: &NidWindow, snapshot: &Snapshot, cx: &mut Context<NidWindow>) -> AnyElement {
    let other = snapshot
        .unplaced
        .iter()
        .find(|computer| copy::placeable(computer))
        .map(|computer| computer.name.clone());
    let note = copy::finish_note(&snapshot.pairing_window, other.as_deref(), &host_name());
    let first = snapshot.unplaced.iter().position(copy::placeable);
    let marks = snapshot
        .unplaced
        .iter()
        .any(|computer| computer.mark.is_some());
    let copied = this.copied;
    frame(
        680.0,
        v_flex()
            .children(problems(snapshot, Some("sharing"), cx))
            .child(title("Pick your other computer"))
            .child(widgets::muted(
                "Open Nidus on it too. Computers on this network show up here by themselves.",
                14.0,
                cx,
            ))
            .child(
                v_flex()
                    .pt(px(14.0))
                    .gap(px(10.0))
                    .map(|list| {
                        if snapshot.unplaced.is_empty() {
                            list.child(
                                h_flex()
                                    .justify_center()
                                    .gap(px(10.0))
                                    .py(px(24.0))
                                    .child(Spinner::new())
                                    .child(widgets::muted(
                                        "Looking for computers on this network…",
                                        13.0,
                                        cx,
                                    )),
                            )
                        } else {
                            list.children(snapshot.unplaced.iter().enumerate().map(
                                |(index, computer)| found(computer, Some(index) == first, cx),
                            ))
                        }
                    }),
            )
            .when(marks, |column| {
                column.child(widgets::muted(
                    "The colored mark also shows on the other computer, so you can tell two with the same name apart.",
                    12.0,
                    cx,
                ))
            })
            .when_some(note, |column, (title, detail)| {
                column.child(widgets::note(title, detail, cx))
            })
            .child(div().flex_1())
            .child(
                h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .pl(px(12.0))
                    .pr(px(6.0))
                    .py(px(6.0))
                    .surface(Surface::new(penna::roles::SETTINGS_PREVIEW), cx)
                    .child(widgets::muted("Not listed? Run on it:", 12.0, cx).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(rems_from_px(12.0))
                            .child(copy::INSTALL_COMMAND),
                    )
                    .child(
                        Button::new("copy-install")
                            .small()
                            .on(cx.theme().ground_of(penna::roles::SETTINGS_PREVIEW))
                            .label(if copied { "Copied" } else { "Copy" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.write_to_clipboard(ala::ClipboardItem::new_string(
                                    copy::INSTALL_COMMAND.to_owned(),
                                ));
                                this.copied = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("find-by-address")
                            .small()
                            .on(cx.theme().ground_of(penna::roles::SETTINGS_PREVIEW))
                            .label("By address…")
                            .tooltip("For a computer Nidus can’t find on this network")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_add_address(window, cx);
                            })),
                    ),
            ),
    )
}

fn found(computer: &Unplaced, first: bool, cx: &mut Context<NidWindow>) -> AnyElement {
    let placeable = copy::placeable(computer);
    let (id, name, mark) = (
        computer.id.clone(),
        computer.name.clone(),
        computer.mark.clone(),
    );
    h_flex()
        .w_full()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(10.0))
        .surface_at(
            Surface::new(SHELF_TILE),
            if placeable {
                PaintState::Rest
            } else {
                PaintState::Disabled
            },
            cx,
        )
        .child(
            Icon::new(if computer.os == Some(nid::wire::Os::Macos) {
                IconName::Laptop
            } else {
                IconName::Server
            })
            .with_size(px(18.0)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .truncate()
                        .text_size(rems_from_px(14.0))
                        .font_medium()
                        .child(computer.name.clone()),
                )
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .child(widgets::muted(copy::unplaced_detail(computer), 12.0, cx))
                        .children(computer.mark.as_deref().map(widgets::key_mark)),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("pick-{}", computer.id)))
                .label("This one")
                .when(first, ButtonVariants::primary)
                .disabled(!placeable)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.board.read(cx).place(&id, Side::Right, cx);
                    this.first_run = Some(Step::Place {
                        id: id.clone(),
                        name: name.clone(),
                        mark: mark.clone(),
                    });
                    cx.notify();
                })),
        )
        .into_any_element()
}

fn placed<'a>(snapshot: &'a Snapshot, name: &str, mark: Option<&str>) -> Option<&'a Peer> {
    snapshot.peers.iter().find(|peer| match mark {
        Some(mark) => peer.mark == mark,
        None => peer.name == name,
    })
}

fn place(
    this: &NidWindow,
    snapshot: &Snapshot,
    id: &str,
    name: &str,
    mark: Option<&str>,
    cx: &mut Context<NidWindow>,
) -> AnyElement {
    let peer = placed(snapshot, name, mark);
    let side = peer
        .and_then(|peer| board::neighbor(snapshot).filter(|(beside, _)| *beside == peer.name))
        .map_or(Side::Right, |(_, side)| Side::of_crossing(side));
    let owner = peer.map(|peer| format!("peer:{}", peer.name));
    let (board, found) = (this.board.clone(), id.to_owned());
    let picker = SegmentedControl::new("place-side", Side::ALL.map(Side::label))
        .selected(Side::ALL.iter().position(|each| *each == side).unwrap_or(1))
        .on_select(move |index, _, cx| {
            let side = Side::ALL[index];
            match &owner {
                Some(owner) => board.read(cx).put(owner, side, cx),
                None => board.read(cx).place(&found, side, cx),
            }
        });
    let hint = match peer {
        None => format!("Adding {name}…"),
        Some(peer) if copy::linked(peer.state) => "You can move it again any time.".to_owned(),
        Some(peer) => format!(
            "{} is not answering yet. If it stays that way, add {} from Nidus on {}.",
            peer.name,
            host_name(),
            peer.name
        ),
    };
    let added = peer.map(|peer| peer.name.clone());
    let forget = added.clone();
    frame(
        800.0,
        v_flex()
            .child(
                h_flex()
                    .items_end()
                    .gap(px(16.0))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.0))
                            .child(title(format!("Where is {name}?")))
                            .child(widgets::muted(
                                "Drag it next to your screens, the way it sits on your desk.",
                                14.0,
                                cx,
                            )),
                    )
                    .child(picker),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .my(px(12.0))
                    .child(this.board.clone()),
            )
            .child(footer(
                Button::new("place-back")
                    .label("Back")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(name) = &forget {
                            this.send(Request::Forget { name: name.clone() }, cx);
                        }
                        this.first_run = Some(Step::Find);
                        cx.notify();
                    })),
                hint,
                Button::new("place-done")
                    .primary()
                    .label("Done")
                    .disabled(added.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(name) = added.clone() {
                            this.first_run = Some(Step::Try {
                                name,
                                reached: false,
                                back: false,
                            });
                            cx.notify();
                        }
                    })),
                cx,
            )),
    )
}

fn try_it(
    this: &NidWindow,
    snapshot: &Snapshot,
    name: &str,
    reached: bool,
    back: bool,
    cx: &mut Context<NidWindow>,
) -> AnyElement {
    let peer = snapshot.peers.iter().find(|peer| peer.name == name);
    let linked = peer.is_some_and(|peer| copy::linked(peer.state));
    let side = board::neighbor(snapshot)
        .filter(|(beside, _)| beside == name)
        .map(|(_, side)| side);
    let line = match (linked, side) {
        (false, _) => format!("Waiting for {name} to connect…"),
        (true, Some(side)) => format!(
            "Push your pointer off the {} edge of your screen.",
            side.to_lowercase()
        ),
        (true, None) => format!("Put {name} next to your screens first."),
    };
    let allowed = snapshot
        .platform
        .as_ref()
        .and_then(|platform| platform.accessibility);
    let checks: Vec<(String, bool)> = allowed
        .map(|on| (format!("Allowed on {}", copy::THIS_COMPUTER_LOWER), on))
        .into_iter()
        .chain([
            (format!("{name} placed"), peer.is_some()),
            (format!("Reach {name}"), reached),
            ("Come back".to_owned(), back),
        ])
        .collect();
    let current = checks.iter().position(|(_, done)| !done);
    frame(
        800.0,
        v_flex()
            .child(title("Try it"))
            .child(widgets::muted(line, 14.0, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .gap(px(24.0))
                    .my(px(14.0))
                    .child(div().flex_1().h_full().child(this.board.clone()))
                    .child(
                        v_flex()
                            .w(px(250.0))
                            .flex_none()
                            .gap(px(14.0))
                            .pt(px(8.0))
                            .children(checks.into_iter().enumerate().map(
                                |(index, (label, done))| {
                                    h_flex()
                                        .gap(px(10.0))
                                        .text_size(rems_from_px(14.0))
                                        .child(Check::checkbox(done))
                                        .child(
                                            div()
                                                .when(current == Some(index), |this| {
                                                    this.font_medium()
                                                })
                                                .child(label),
                                        )
                                },
                            ))
                            .child(
                                v_flex()
                                    .mt(px(8.0))
                                    .gap(px(8.0))
                                    .px(px(12.0))
                                    .py(px(10.0))
                                    .surface(Surface::new(penna::roles::SETTINGS_PREVIEW), cx)
                                    .child(widgets::muted(
                                        "Stuck over there? This always brings you home:",
                                        12.0,
                                        cx,
                                    ))
                                    .child(h_flex().child(
                                        Keycap::new(copy::RETURN_KEYS).on(
                                            cx.theme().ground_of(penna::roles::SETTINGS_PREVIEW),
                                        ),
                                    )),
                            ),
                    ),
            )
            .child(footer(
                div(),
                copy::STAYS_RUNNING,
                Button::new("try-finish")
                    .label(if back { "Finish" } else { "Skip" })
                    .when(back, ButtonVariants::primary)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.first_run = None;
                        this.board.update(cx, |board, cx| board.select(None, cx));
                        cx.notify();
                    })),
                cx,
            )),
    )
}

pub fn added_by(
    this: &NidWindow,
    snapshot: &Snapshot,
    notice: &Notice,
    cx: &mut Context<NidWindow>,
) -> AnyElement {
    this.board
        .update(cx, |board, cx| board.set_docked(false, cx));
    let name = notice.name.clone();
    let forget = name.clone();
    frame(
        680.0,
        v_flex()
            .child(
                h_flex()
                    .items_start()
                    .gap(px(20.0))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.0))
                            .child(title(format!("{name} added {}", copy::THIS_COMPUTER_LOWER)))
                            .child(widgets::muted(
                                "Its keyboard and pointer can come over now. Nothing to do here.",
                                14.0,
                                cx,
                            )),
                    )
                    .children(snapshot.own_mark.as_deref().map(|mark| {
                        v_flex()
                            .flex_none()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(16.0))
                            .py(px(10.0))
                            .surface(Surface::new(penna::roles::SETTINGS_PREVIEW), cx)
                            .child(widgets::muted("Mark", 12.0, cx))
                            .child(widgets::key_mark_sized(mark, 14.0))
                    })),
            )
            .when(snapshot.own_mark.is_some(), |column| {
                column.child(widgets::muted(
                    format!("Same mark as on {name}’s screen? Then it’s the right one."),
                    13.0,
                    cx,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .my(px(16.0))
                    .child(this.board.clone()),
            )
            .child(footer(
                Button::new("not-mine")
                    .with_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Ghost))
                    .label(format!("Not mine, forget {name}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(
                            Request::Forget {
                                name: forget.clone(),
                            },
                            cx,
                        );
                        this.model.update(cx, Model::welcomed);
                    })),
                format!("Placed on {name}. Move it here or there, both stay in sync."),
                Button::new("looks-right")
                    .primary()
                    .label("Looks right")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, Model::welcomed);
                    })),
                cx,
            )),
    )
}

#[cfg(test)]
mod tests {
    use nid::app::api::State;

    use super::{Step, next, watch};
    use crate::board::tests::snapshot;

    #[test]
    fn first_run_asks_first_then_finds_and_ends_once_a_computer_is_added() {
        let mut alone = snapshot();
        alone.peers.clear();
        assert_eq!(next(None, &alone, true), Some(Step::Allow));
        assert_eq!(next(Some(Step::Allow), &alone, false), Some(Step::Find));
        assert_eq!(next(Some(Step::Find), &alone, true), Some(Step::Find));
        assert_eq!(next(None, &alone, false), Some(Step::Find));

        let paired = snapshot();
        assert_eq!(next(None, &paired, true), None);
        assert_eq!(next(Some(Step::Find), &paired, false), None);
        let placing = Step::Place {
            id: "key:mac".into(),
            name: "mac".into(),
            mark: Some("996615".into()),
        };
        assert_eq!(next(Some(placing.clone()), &paired, false), Some(placing));
    }

    #[test]
    fn trying_it_waits_for_a_trip_over_and_back() {
        let trying = |reached, back| {
            Some(Step::Try {
                name: "mac".into(),
                reached,
                back,
            })
        };
        let mut step = trying(false, false);
        watch(&mut step, State::Ready);
        assert_eq!(step, trying(false, false));
        watch(&mut step, State::Controlling);
        assert_eq!(step, trying(true, false));
        watch(&mut step, State::Controlling);
        assert_eq!(step, trying(true, false));
        watch(&mut step, State::Ready);
        assert_eq!(step, trying(true, true));
    }
}
