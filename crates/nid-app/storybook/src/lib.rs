mod fixtures;
mod stories;

use penna_storybook::Catalog;

pub static CATALOG: Catalog = Catalog {
    title: "Nidus storybook",
    name: "nidus",
    storage: "nidus-storybook",
    init: nid_app::theme::init,
    logo: penna::IconName::Laptop.path(),
    plates: &[],
    stories: &[
        penna_storybook::FOUNDATION,
        stories::STORIES,
        penna_storybook::KIT,
    ],
    knobs: &[],
    apply: |_, _| {},
    floating: &[penna_storybook::backdrop::KIT_FLOATING, stories::FLOATING],
};

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run() -> Result<(), wasm_bindgen::JsValue> {
    penna_storybook::start(&CATALOG, penna::Assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_has_no_problems() {
        penna_storybook::install(&CATALOG);
        assert_eq!(CATALOG.problems(), Vec::<String>::new());
    }
}
