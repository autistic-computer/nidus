pub mod board;
pub mod copy;
mod lens;
pub mod model;
pub mod pages;
pub mod panel;
pub mod platform;
pub mod theme;
#[cfg(any(target_os = "macos", windows))]
mod tray;
pub mod widgets;
pub mod window;

use std::path::PathBuf;

use ala::{
    App, AppContext as _, Bounds, Entity, SystemNotification, SystemNotificationAction,
    WindowBounds, WindowDecorations, WindowHandle, WindowOptions, px, size,
};
use penna::{Root, TitleBar};

use crate::{
    model::{Model, Page},
    window::NidWindow,
};

pub const APP_ID: &str = "io.nid.nid";
const FORGET: &str = "forget";

#[derive(Clone, Debug, Default)]
pub struct Arguments {
    pub config: Option<PathBuf>,
    pub show_settings: bool,
    pub background: bool,
    pub quit: bool,
    pub replace_process: Option<u32>,
    pub replace_user: Option<String>,
    pub restart_ready: Option<String>,
}

impl Arguments {
    pub fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self::default();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--config" => {
                    parsed.config = Some(arguments.next().ok_or("--config takes a path")?.into());
                }
                "--show-settings" => parsed.show_settings = true,
                "--background" => parsed.background = true,
                "--quit" => parsed.quit = true,
                "--replace-process" => {
                    let pid = arguments
                        .next()
                        .ok_or("--replace-process takes a process id")?;
                    parsed.replace_process = Some(
                        pid.parse()
                            .map_err(|_| "--replace-process takes a process id")?,
                    );
                }
                "--replace-user" => {
                    parsed.replace_user =
                        Some(arguments.next().ok_or("--replace-user takes an id")?);
                }
                "--restart-ready" => {
                    parsed.restart_ready =
                        Some(arguments.next().ok_or("--restart-ready takes a name")?);
                }
                argument if argument.starts_with("-psn_") => {}
                argument => return Err(format!("unknown argument: {argument}")),
            }
        }
        Ok(parsed)
    }
}

pub fn open(cx: &mut App) {
    if let Some(window) = cx.try_global::<MainWindow>().and_then(|window| window.0)
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        platform::window_shown(cx);
        cx.activate(true);
        return;
    }
    let model = model::model(cx);
    let Some(services) = cx
        .try_global::<platform::SharedServices>()
        .map(|services| services.0.clone())
    else {
        return;
    };
    let bounds = Bounds::centered(None, size(px(960.0), px(680.0)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitleBar::title_bar_options(
                penna::Theme::global(cx).title_bar_height(),
            )),
            window_min_size: Some(size(px(700.0), px(480.0))),
            window_decorations: Some(if cx.compositor_name() == "X11" {
                WindowDecorations::Server
            } else {
                WindowDecorations::Client
            }),
            app_id: Some(APP_ID.into()),
            ..Default::default()
        },
        |window, cx| {
            theme::apply(Some(window), cx);
            window
                .observe_window_appearance(|window, cx| theme::apply(Some(window), cx))
                .detach();
            window.set_window_title("Nidus");
            window.on_window_should_close(cx, |_, cx| platform::may_close(cx));
            let view = cx.new(|cx| NidWindow::new(model, services, window, cx));
            cx.new(|cx| Root::new(view, window, cx).bordered(cfg!(target_os = "linux")))
        },
    );
    match opened {
        Ok(handle) => {
            cx.set_global(MainWindow(Some(handle)));
            platform::window_shown(cx);
            cx.activate(true);
        }
        Err(error) => log::error!("Nidus could not open its window: {error:#}"),
    }
}

pub struct MainWindow(pub Option<WindowHandle<Root>>);

impl ala::Global for MainWindow {}

pub fn notify_notices(model: &Entity<Model>, cx: &mut App) {
    cx.observe(model, |model, cx| {
        let notices = model.update(cx, |model, _| model.take_notices());
        for notice in notices {
            let first =
                model.read(cx).snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.peers.iter().all(|peer| peer.name == notice.name)
                });
            if first {
                model.update(cx, |model, cx| model.welcome(notice, cx));
                open(cx);
                continue;
            }
            cx.show_system_notification(SystemNotification {
                tag: format!("joined:{}", notice.name).into(),
                title: format!("{} joined", notice.name).into(),
                body: format!(
                    "Its mark is {}. It can share {}’s keyboard and mouse. Not yours?",
                    notice.mark,
                    copy::THIS_COMPUTER_LOWER
                )
                .into(),
                actions: vec![SystemNotificationAction {
                    id: FORGET.into(),
                    label: "Forget".into(),
                }],
            });
        }
    })
    .detach();
    cx.on_system_notification_response(|response, cx| {
        if let Some(name) = response.tag.strip_prefix("joined:") {
            let model = model::model(cx);
            if response.action_id.as_deref() == Some(FORGET) {
                model.read(cx).send(nid::app::api::Request::Forget {
                    name: name.to_owned(),
                });
            } else {
                model.update(cx, |model, cx| {
                    model.show(Page::Computer(name.to_owned()), cx);
                });
                open(cx);
            }
        } else {
            platform::notification_response(&response, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_read_as_the_native_apps_took_them() {
        let parse = |words: &[&str]| Arguments::parse(words.iter().map(|word| (*word).to_owned()));
        let parsed = parse(&["--config", "/tmp/z.toml", "--show-settings"]).unwrap();
        assert_eq!(parsed.config, Some(PathBuf::from("/tmp/z.toml")));
        assert!(parsed.show_settings && !parsed.background && !parsed.quit);
        assert!(parse(&["--background", "-psn_0_1234"]).unwrap().background);
        assert!(parse(&["--config"]).is_err());
        assert!(parse(&["--sparkle"]).is_err());
    }
}
