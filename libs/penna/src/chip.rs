use ala::{
    App, Div, Hsla, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Emphasis, Fill, Ground, Icon, Label, Layer, PaintState, Selectable, Sizable,
    Size, Space, StyledExt as _, Surface, SurfaceRole, TextStyle, Theme, Tone, h_flex,
    rems_from_px,
};

const DOT: f32 = 6.0;

#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    dot: Option<Tone>,
    icon: Option<Icon>,
    detail: Option<SharedString>,
    tone: Tone,
    emphasis: Emphasis,
    selected: bool,
    size: Size,
    refinement: StyleRefinement,
}

impl Chip {
    #[must_use]
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            dot: None,
            icon: None,
            detail: None,
            tone: Tone::Neutral,
            emphasis: Emphasis::Soft,
            selected: false,
            size: Size::Medium,
            refinement: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn dot(mut self, tone: Tone) -> Self {
        self.dot = Some(tone);
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        self.emphasis = emphasis;
        self
    }

    fn paint_layer(&self, theme: &Theme) -> Layer {
        let role = SurfaceRole::CHIP;
        let role = match self.emphasis {
            Emphasis::Solid | Emphasis::Soft => role,
            Emphasis::Outline => role.with(role.patch.fill(Fill::None)),
            Emphasis::Ghost => role.with(role.patch.fill(Fill::None).outline(false)),
        };
        let surface = Surface::new(role)
            .tone(self.tone)
            .prominent(self.emphasis == Emphasis::Solid);
        let state = if self.selected {
            PaintState::Selected
        } else {
            PaintState::Rest
        };
        theme.layer(surface, state)
    }
}

impl Selectable for Chip {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Sizable for Chip {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Chip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let layer = self.paint_layer(cx.theme());
        let ground = layer.ground;
        let (height, padding, text) = match self.size {
            Size::Small => (20.0, Space::S4, TextStyle::Caption),
            Size::Medium | Size::Size(_) => (24.0, Space::S5, TextStyle::Small),
        };
        h_flex()
            .min_w_0()
            .h(rems_from_px(height))
            .px(padding)
            .gap(Space::S3)
            .layer(&layer)
            .refine_style(&self.refinement)
            .when_some(self.dot, |chip, tone| chip.child(dot(tone, ground, cx)))
            .when_some(self.icon, |chip, icon| {
                chip.child(icon.with_size(px(text.design_px() + 1.0)))
            })
            .child(
                Label::new(text, self.label)
                    .strong()
                    .flex_none()
                    .whitespace_nowrap(),
            )
            .when_some(self.detail, |chip, detail| {
                chip.child(
                    Label::new(text, format!("· {detail}"))
                        .muted()
                        .on(ground)
                        .min_w_0()
                        .truncate(),
                )
            })
    }
}

pub(crate) fn dot(tone: Tone, ground: Ground, cx: &App) -> Div {
    div()
        .flex_none()
        .size(rems_from_px(DOT))
        .rounded_full()
        .bg(dot_color(tone, ground, cx))
}

fn dot_color(tone: Tone, ground: Ground, cx: &App) -> Hsla {
    let theme = cx.theme();
    match tone {
        Tone::Neutral => theme
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest),
        tone => theme.tone(tone),
    }
}
