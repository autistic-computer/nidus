use ala::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use nid::app::api::Level;
use penna::v_flex;

use super::{card, first_run, fix_button};
use crate::{copy, model::Snapshot, platform, widgets, window::NidWindow};

pub fn render(
    this: &mut NidWindow,
    window: &mut Window,
    cx: &mut Context<NidWindow>,
) -> AnyElement {
    let model = this.model.read(cx);
    let (Some(snapshot), added_by) = (model.snapshot.clone(), model.added_by.clone()) else {
        return div().into_any_element();
    };
    if let Some(notice) = added_by {
        return first_run::added_by(this, &snapshot, &notice, cx);
    }
    if (snapshot.peers.is_empty() || this.first_run.is_some())
        && let Some(page) = first_run::render(this, &snapshot, window, cx)
    {
        return page;
    }
    this.board
        .update(cx, |board, cx| board.set_docked(true, cx));
    let problems = problems(&snapshot, None, cx);
    let card = card::render(this, &snapshot, window, cx);
    div()
        .flex()
        .size_full()
        .min_h_0()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .when(!problems.is_empty(), |column| {
                    column.child(v_flex().p(px(12.0)).gap(px(8.0)).children(problems))
                })
                .child(div().flex_1().min_h_0().child(this.board.clone())),
        )
        .children(card)
        .into_any_element()
}

pub(super) fn problems(
    snapshot: &Snapshot,
    skip: Option<&str>,
    cx: &mut Context<NidWindow>,
) -> Vec<AnyElement> {
    snapshot
        .health
        .iter()
        .filter(|row| row.level != Level::Ok && Some(row.id.as_str()) != skip)
        .map(|row| {
            let action = platform::health_action(row, cx).map(|action| {
                fix_button(&row.id, &action, widgets::banner_ground(row.level, cx), cx)
            });
            widgets::banner(row.level, row.title.clone(), row.detail.clone(), action)
                .into_any_element()
        })
        .collect()
}

pub fn host_name() -> String {
    let name = if cfg!(target_family = "wasm") {
        String::new()
    } else {
        nid::hello::local_name()
    };
    if name.is_empty() {
        copy::THIS_COMPUTER_LOWER.to_owned()
    } else {
        name
    }
}
