mod board;
mod card;
mod computers;
mod first_run;
mod panel;
mod settings;
mod titlebar;
mod widgets;

use penna_storybook::Story;

pub const STORIES: &[Story] = &[
    first_run::STORY,
    computers::STORY,
    card::STORY,
    settings::STORY,
    panel::STORY,
    board::STORY,
    titlebar::STORY,
    widgets::STORY,
];

pub const FLOATING: &[&str] = &[
    "first-run/add-address",
    "first-run/add-address-error",
    "first-run/add-address-looking",
    "computers/failure-toast",
    "card/keys-menu",
    "card/forget-confirm",
];
