import 'common.just'

set working-directory := ".."

[default]
[private]
default:
    @just --list package

[doc("Pack the Mac app into target/release/nid.dmg, signed like the app but not notarized.")]
dmg *args: (_platform "mac")
    ./scripts/build-macos-app.sh --dmg "$@"

[doc("Pack the GNOME extension for extensions.gnome.org under target/dist.")]
extension: (_platform "linux")
    ./scripts/pack-extension.sh
