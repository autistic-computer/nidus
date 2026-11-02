#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use ala::{App, AppContext as _, QuitMode};
use nid_app::{
    APP_ID, Arguments,
    model::{Model, Page, Shared},
    notify_notices, platform, theme,
};

fn main() {
    #[cfg(target_os = "windows")]
    platform::velopack_hooks();
    let arguments = match Arguments::parse(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("nid-app: {error}");
            std::process::exit(2);
        }
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,nid=info".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    if !platform::before_start(&arguments) {
        return;
    }
    let config = arguments
        .config
        .clone()
        .unwrap_or_else(platform::default_config);
    let application = ala_platform::application();
    #[cfg(target_os = "macos")]
    application.on_reopen(nid_app::open);
    application
        .with_assets(penna::Assets)
        .with_quit_mode(if platform::LIVES_IN_TRAY {
            QuitMode::Explicit
        } else {
            QuitMode::LastWindowClosed
        })
        .run(move |cx: &mut App| {
            cx.set_app_identity(APP_ID, "Nidus");
            theme::init(cx);
            let model = cx.new(|cx| Model::new(config, cx));
            cx.set_global(Shared(model.clone()));
            if arguments.show_settings {
                model.update(cx, |model, cx| model.show(Page::Settings, cx));
            }
            let services = platform::init(cx);
            notify_notices(&model, cx);
            platform::start(&model, &services, &arguments, cx);
        });
}
