use zz_gpui::{App, AppContext as _, Bounds, Context, IntoElement, ParentElement as _, Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size};
use zz_gpui_kit::{ActiveTheme as _, Root, switch::Switch};

struct Hello {
    on: bool,
}

impl Render for Hello {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_8()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child("zflow on zz-gpui")
            .child(
                Switch::new("sharing")
                    .checked(self.on)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.on = *checked;
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    zz_gpui_platform::application()
        .with_assets(zz_gpui_kit::Assets)
        .run(|cx: &mut App| {
            zz_gpui_kit::init(cx);
            let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    app_id: Some("io.zflow.zflow".into()),
                    ..Default::default()
                },
                |window, cx| {
                    zz_gpui_kit::theme::Theme::sync_system_appearance(Some(window), cx);
                    let view = cx.new(|_| Hello { on: true });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("the window opens");
        });
}
