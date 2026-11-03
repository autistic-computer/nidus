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
    Disableable as _, Emphasis, Icon, IconName, Label, PaintState, Sizable as _, Steps, StyledExt,
    TextStyle, Tile, Tone, Well,
    components::button::{Button, ButtonVariant, ButtonVariants},
    components::check::Check,
    components::kbd::Keycap,
    components::segmented::SegmentedControl,
    components::spinner::Spinner,
    h_flex, v_flex,
};

use super::computers::{host_name, problems};
use crate::{
    board::{self, geometry::Side},
    copy,
    model::{Model, Snapshot},
    platform, widgets,
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
        Steps::new(STEPS.iter().copied())
            .current(now)
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

fn title(text: impl Into<SharedString>) -> Label {
    Label::new(TextStyle::Display, text)
}

fn footer(
    left: impl IntoElement,
    hint: impl Into<SharedString>,
    right: impl IntoElement,
) -> ala::Div {
    h_flex()
        .w_full()
        .gap(px(10.0))
        .child(left)
        .child(
            Label::new(TextStyle::Body, hint)
                .muted()
                .flex_1()
                .text_center(),
        )
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
            .child(
                Label::new(
                    TextStyle::Lead,
                    "macOS asks once for each. You can change them later in System Settings.",
                )
                .muted(),
            )
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
            .child(Label::new(TextStyle::Lead, "Open Nidus on it too. Computers on this network show up here by themselves.").muted())
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
                                    .child(Label::new(TextStyle::Body, "Looking for computers on this network…").muted()),
                            )
                        } else {
                            list.children(snapshot.unplaced.iter().enumerate().map(
                                |(index, computer)| found(computer, Some(index) == first, cx),
                            ))
                        }
                    }),
            )
            .when(marks, |column| {
                column.child(Label::new(TextStyle::Small, "The colored mark also shows on the other computer, so you can tell two with the same name apart.").muted())
            })
            .when_some(note, |column, (title, detail)| {
                column.child(widgets::note(title, detail))
            })
            .child(div().flex_1())
            .child(
                Well::new()
                    .h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .pl(px(12.0))
                    .pr(px(6.0))
                    .py(px(6.0))
                    .child(Label::new(TextStyle::Small, "Not listed? Run on it:").muted().flex_none())
                    .child(
                        Label::new(TextStyle::Mono, copy::INSTALL_COMMAND)
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        Button::new("copy-install")
                            .small()
                            .on(Well::ground(cx))
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
                            .on(Well::ground(cx))
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
    Tile::new(SharedString::from(format!("found-row-{}", computer.id)))
        .state(if placeable {
            PaintState::Rest
        } else {
            PaintState::Disabled
        })
        .h_flex()
        .w_full()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(10.0))
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
                    Label::new(TextStyle::Lead, computer.name.clone())
                        .strong()
                        .truncate(),
                )
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .child(
                            Label::new(TextStyle::Small, copy::unplaced_detail(computer)).muted(),
                        )
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
                            .child(
                                Label::new(
                                    TextStyle::Lead,
                                    "Drag it next to your screens, the way it sits on your desk.",
                                )
                                .muted(),
                            ),
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
            .child(Label::new(TextStyle::Lead, line).muted())
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
                                    let label = Label::new(TextStyle::Lead, label);
                                    h_flex().gap(px(10.0)).child(Check::checkbox(done)).child(
                                        if current == Some(index) {
                                            label.strong()
                                        } else {
                                            label
                                        },
                                    )
                                },
                            ))
                            .child(
                                Well::new()
                                    .v_flex()
                                    .mt(px(8.0))
                                    .gap(px(8.0))
                                    .px(px(12.0))
                                    .py(px(10.0))
                                    .child(
                                        Label::new(
                                            TextStyle::Small,
                                            "Stuck over there? This always brings you home:",
                                        )
                                        .muted(),
                                    )
                                    .child(h_flex().child(
                                        Keycap::new(copy::RETURN_KEYS).on(Well::ground(cx)),
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
                            .child(Label::new(TextStyle::Lead, "Its keyboard and pointer can come over now. Nothing to do here.").muted()),
                    )
                    .children(snapshot.own_mark.as_deref().map(|mark| {
                        Well::new()
                            .v_flex()
                            .flex_none()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(16.0))
                            .py(px(10.0))
                            .child(Label::new(TextStyle::Small, "Mark").muted())
                            .child(widgets::key_mark_sized(mark, 14.0))
                    })),
            )
            .when(snapshot.own_mark.is_some(), |column| {
                column.child(Label::new(TextStyle::Body, format!("Same mark as on {name}’s screen? Then it’s the right one.")).muted())
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
                    })))),
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
