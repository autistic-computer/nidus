use ala::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Edge, Ground, Label, PaintState, Space, StyledExt as _, Surface, SurfaceRole,
    TextStyle, Tone, h_flex, roles, v_flex,
};

#[derive(IntoElement, Default)]
pub struct Well {
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Well {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn ground(cx: &App) -> Ground {
        cx.theme().ground_of(SurfaceRole::WELL)
    }
}

impl ParentElement for Well {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Well {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Well {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .surface(SurfaceRole::WELL, cx)
            .refine_style(&self.refinement)
            .children(self.children)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileVariant {
    #[default]
    Card,
    Screen,
}

impl TileVariant {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Card => SurfaceRole::CARD_TILE,
            Self::Screen => SurfaceRole::SCREEN_TILE,
        }
    }
}

#[derive(IntoElement)]
pub struct Tile {
    id: ElementId,
    variant: TileVariant,
    tone: Tone,
    state: PaintState,
    ground: Option<Ground>,
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Tile {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: TileVariant::Card,
            tone: Tone::Neutral,
            state: PaintState::Rest,
            ground: None,
            refinement: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }

    #[must_use]
    pub fn variant(mut self, variant: TileVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.state = state;
        self
    }
}

impl ParentElement for Tile {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Tile {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Tile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .surface_at(
                Surface::new(self.variant.role())
                    .tone(self.tone)
                    .on_maybe(self.ground),
                self.state,
                cx,
            )
            .refine_style(&self.refinement)
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct Dock {
    title: SharedString,
    subtitle: Option<SharedString>,
    action: Option<AnyElement>,
    header_height: Option<Pixels>,
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Dock {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            action: None,
            header_height: None,
            refinement: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    #[must_use]
    pub fn header_height(mut self, height: Pixels) -> Self {
        self.header_height = Some(height);
        self
    }
}

impl ParentElement for Dock {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Dock {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Dock {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .rule_on(roles::BAR_RULE, Edge::Top, cx)
            .refine_style(&self.refinement)
            .child(
                h_flex()
                    .when_some(self.header_height, Styled::h)
                    .px(Space::S8)
                    .gap(Space::S6)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(TextStyle::Body, self.title).strong())
                            .children(
                                self.subtitle
                                    .map(|subtitle| Label::new(TextStyle::Small, subtitle).muted()),
                            ),
                    )
                    .children(self.action),
            )
            .children(self.children)
    }
}
