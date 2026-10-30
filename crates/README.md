# crates

Nidus's app.

| Crate | What it is |
|---|---|
| `nid-app` | The app: one window with the computers and the settings, a menu bar or tray icon, and on a Mac the engine itself. The same code runs on Linux, macOS and Windows. |

The app is built on [ala](https://github.com/autistic-computer/ala), a fork of Zed's GPUI, and [penna](https://github.com/autistic-computer/penna), its widget kit. Both come from their `main` branches on GitHub, and `Cargo.lock` pins the revisions. penna's `tray` feature draws the menu bar or tray icon. The app follows zz's design language: colors come from the theme, surfaces from roles, and it draws penna's Default interface theme in the system's light or dark mode. Nidus's own roles and sizes live in `nid-app`, and its `clippy.toml` keeps raw colors and role-less surfaces out.

## Checking other platforms from Linux

`cargo check` and `cargo clippy` for Windows and macOS run on Linux with a stand-in C compiler, since nothing links:

- Windows: `--target x86_64-pc-windows-gnu`, `RC_x86_64_pc_windows_gnu=llvm-rc`, and a `CC_x86_64_pc_windows_gnu` that preprocesses with `clang` (`clang --driver-mode=cl` for the resource file) and writes empty objects otherwise.
- macOS: `--target aarch64-apple-darwin` and the same stand-in as `CC_aarch64_apple_darwin`.

Real builds still need each system: Xcode on a Mac, the Windows SDK's `fxc.exe` on Windows for release shaders.
