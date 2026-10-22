# crates

zflow's app and the UI stack it is built on.

| Crate | What it is |
|---|---|
| `zflow-app` | The app: one window with the computers and the settings, a menu bar or tray icon, and on a Mac the engine itself. The same code runs on Linux, macOS and Windows. |
| `zz-gpui`, `zz-gpui-macros`, `zz-gpui-platform` | zz's fork of Zed's GPUI, vendored. |
| `zz-gpui-kit` | zz's widget kit, vendored, with the additions listed below. |
| `zz-gpui-tray` | New: a tray or menu bar icon with a menu, for any zz-gpui app. |

The zz-gpui crates were copied from [demfabris/zz](https://github.com/demfabris/zz) at `32ee3ab37`. They follow zz's design language: colors come from the theme, surfaces from roles, and the app draws zz's Default interface theme in the system's light or dark mode.

## Backport to zz

Everything here that is not zflow's goes back to zz. Each item is self-contained and depends on nothing in zflow.

### zz-gpui-kit

- `roles`: the window chrome's surface roles (window, title bar, sidebar, settings cards and previews, chrome and window-control buttons, the bar, sidebar and list rules), moved out of zz-ui's `roles.rs`, and `register` for an app's own roles. zz-ui keeps its zz roles and registers them; its `all()` becomes the kit's.
- `interface_theme`: zz-ui's `interface_theme.rs` with the Default theme built in and role checks against the kit's registry. zz-ui keeps Liquid Glass (its roles are zz's) and the appearance settings that live in zz-client.
- `settings`: zz-ui's settings page pieces (`SettingsStack`, `SettingEntry`, `SettingCopy`, `StackPosition`, the page heading, scroll column and group headers) and navigation rows that take a title and icon instead of zz's `SettingsSection`.
- `sidebar`: zz-ui's sidebar panel, title strip, footer, resize handle, chrome button and status dot, without the `workspace_` prefix.
- `shell`: zz-ui's `app_shell_surface`, `app_titlebar_strip` (now with a leading slot) and `chrome_background`, and zz's `window_drag_handle` from `crates/zz/src/window/drag.rs`.

### zz-gpui-platform

- `build.rs` writes empty Metal outputs when a non-Mac host checks a Mac build, so `cargo check --target aarch64-apple-darwin` works from Linux. Nothing built that way can run.

### zz-gpui-tray

A new crate from zz's `crates/zz/src/tray` backends (ksni, `NSStatusItem`, `Shell_NotifyIcon`), made generic: the app hands it a `Menu` of items with actions and check marks and reads `TrayEvent`s. zz's tray becomes its `facts.rs` menu plus this crate.

## Checking other platforms from Linux

`cargo check` and `cargo clippy` for Windows and macOS run on Linux with a stand-in C compiler, since nothing links:

- Windows: `--target x86_64-pc-windows-gnu`, `RC_x86_64_pc_windows_gnu=llvm-rc`, and a `CC_x86_64_pc_windows_gnu` that preprocesses with `clang` (`clang --driver-mode=cl` for the resource file) and writes empty objects otherwise.
- macOS: `--target aarch64-apple-darwin` and the same stand-in as `CC_aarch64_apple_darwin`.

Real builds still need each system: Xcode on a Mac, the Windows SDK's `fxc.exe` on Windows for release shaders.
