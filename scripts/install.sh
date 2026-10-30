#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly SCRIPT_DIR REPO_ROOT
readonly BIN_DIR="/usr/local/bin"
readonly LIB_DIR="/usr/local/lib/nid"
readonly UNINSTALLER="$LIB_DIR/uninstall.sh"
readonly UNIT_FILE="/etc/systemd/system/nidd.service"
readonly CONFIG_FILE="/etc/nid/nid.toml"
readonly DROPIN_DIR="/etc/systemd/system/nidd.service.d"
readonly PRELOGIN_DROPIN="$DROPIN_DIR/prelogin.conf"
readonly UDEV_RULE_DIR="/etc/udev/rules.d"
readonly VIRTUAL_RULE_FILE="$UDEV_RULE_DIR/70-nid.rules"
readonly CAPTURE_RULE_FILE="$UDEV_RULE_DIR/71-nid-capture.rules"
readonly MODULE_FILE="/etc/modules-load.d/nid.conf"
readonly SLEEP_HOOK_FILE="/usr/lib/systemd/system-sleep/nid"
readonly FIREWALL_SCRIPT="$LIB_DIR/firewall.sh"
readonly LAUNCHER_FILE="/usr/local/share/applications/io.nid.nid.desktop"
readonly DBUS_SERVICE_FILE="/usr/local/share/dbus-1/services/io.nid.Desktop.service"
readonly AUTOSTART_FILE="/etc/xdg/autostart/io.nid.desktop-agent.desktop"
readonly ICON_THEME_DIR="/usr/local/share/icons/hicolor"
readonly APP_ICON_FILE="$ICON_THEME_DIR/scalable/apps/io.nid.nid.svg"
readonly SYMBOLIC_ICON_FILE="$ICON_THEME_DIR/symbolic/apps/io.nid.nid-symbolic.svg"
install_built=false

die() {
    printf 'install: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

require_regular_source() {
    [[ -f "$1" && ! -L "$1" ]] || die "missing or unsafe source file: $1"
}

refuse_symlink() {
    [[ ! -L "$1" ]] || die "refusing to replace symlink: $1"
}

install_session_file() {
    refuse_symlink "$2"
    install -d -o root -g root -m 0755 "${2%/*}"
    sed 's|/usr/bin/nid|/usr/local/bin/nid|g' "$1" > "$2"
    chown root:root "$2"
    chmod 0644 "$2"
}

build_binaries() {
    local cargo_args=(build --locked --release --manifest-path "$REPO_ROOT/Cargo.toml"
        --target-dir "$REPO_ROOT/target" -p nid-kvm -p nid-app
        --bin nid --bin nidd --bin nid-app)
    printf 'Building nid, nidd and nid-app...\n'
    cargo "${cargo_args[@]}"
}

for argument in "$@"; do
    case "$argument" in
        --install-built) install_built=true ;;
        -h|--help)
            printf 'usage: ./scripts/install.sh [--install-built]\n'
            exit 0
            ;;
        *) die "usage: ./scripts/install.sh" ;;
    esac
done
[[ "$(uname -s)" == "Linux" ]] || die "Linux is required"
if [[ -d "$REPO_ROOT/bin" ]]; then install_built=true; fi

for source in \
    "$SCRIPT_DIR/uninstall.sh" \
    "$REPO_ROOT/packaging/linux/host-setup.sh" \
    "$REPO_ROOT/packaging/linux/firewall.sh" \
    "$REPO_ROOT/packaging/linux/io.nid.nid.desktop" \
    "$REPO_ROOT/packaging/linux/io.nid.Desktop.service" \
    "$REPO_ROOT/packaging/linux/io.nid.desktop-agent.desktop" \
    "$REPO_ROOT/assets/linux/io.nid.nid.svg" \
    "$REPO_ROOT/assets/linux/io.nid.nid-symbolic.svg" \
    "$REPO_ROOT/packaging/config/nid.toml" \
    "$REPO_ROOT/packaging/modules-load.d/nid.conf" \
    "$REPO_ROOT/packaging/system-sleep/nid" \
    "$REPO_ROOT/packaging/systemd/nidd.service" \
    "$REPO_ROOT/packaging/udev/70-nid.rules" \
    "$REPO_ROOT/packaging/udev/71-nid-capture.rules"; do
    require_regular_source "$source"
done

if [[ "$EUID" -ne 0 ]]; then
    require_command sudo
    if [[ "$install_built" == false ]]; then
        require_command cargo
        build_binaries
    fi
    exec sudo -- "$SCRIPT_DIR/install.sh" --install-built
fi

if [[ "$install_built" == false ]]; then
    require_command cargo
    build_binaries
fi

for command in grep install sed systemctl systemd-analyze; do
    require_command "$command"
done

binary_dir="$REPO_ROOT/target/release"
if [[ -d "$REPO_ROOT/bin" ]]; then binary_dir="$REPO_ROOT/bin"; fi
for binary in nid nidd nid-app; do
    require_regular_source "$binary_dir/$binary"
    [[ -x "$binary_dir/$binary" ]] || die "binary is not executable: $binary"
done
for package in nid zflow; do
    if command -v dpkg-query >/dev/null 2>&1 && [[ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)" == 'install ok installed' ]]; then
        die "$package is managed by dpkg. Install the release .deb instead of an archive/source build."
    fi
done
if [[ -f /usr/local/lib/zflow/uninstall.sh && ! -L /usr/local/lib/zflow/uninstall.sh ]]; then
    if [[ -f /etc/udev/rules.d/71-zflow-capture.rules && ! -L /etc/udev/rules.d/71-zflow-capture.rules && ! -e /etc/udev/71-zflow-capture.rules.disabled ]]; then
        mv -- /etc/udev/rules.d/71-zflow-capture.rules /etc/udev/71-zflow-capture.rules.disabled
    fi
    bash /usr/local/lib/zflow/uninstall.sh
fi

refuse_symlink "$UNIT_FILE"
refuse_symlink "$VIRTUAL_RULE_FILE"
refuse_symlink "$MODULE_FILE"
refuse_symlink "$SLEEP_HOOK_FILE"
refuse_symlink "$LIB_DIR"
refuse_symlink "$UNINSTALLER"
refuse_symlink "$FIREWALL_SCRIPT"

install -d -o root -g root -m 0755 \
    "$BIN_DIR" "$LIB_DIR" "$UDEV_RULE_DIR" /etc/modules-load.d /usr/lib/systemd/system-sleep
install -o root -g root -m 0755 "$binary_dir/nid" "$BIN_DIR/nid"
install -o root -g root -m 0755 "$binary_dir/nidd" "$BIN_DIR/nidd"
install -o root -g root -m 0755 "$binary_dir/nid-app" "$BIN_DIR/nid-app"
install -o root -g root -m 0755 "$SCRIPT_DIR/uninstall.sh" "$UNINSTALLER"
install -o root -g root -m 0755 "$REPO_ROOT/packaging/linux/firewall.sh" "$FIREWALL_SCRIPT"
rm -f -- "$BIN_DIR/nid-gui"
install_session_file "$REPO_ROOT/packaging/linux/io.nid.nid.desktop" "$LAUNCHER_FILE"
install_session_file "$REPO_ROOT/packaging/linux/io.nid.Desktop.service" "$DBUS_SERVICE_FILE"
install_session_file "$REPO_ROOT/packaging/linux/io.nid.desktop-agent.desktop" "$AUTOSTART_FILE"
refuse_symlink "$APP_ICON_FILE"
refuse_symlink "$SYMBOLIC_ICON_FILE"
install -d -o root -g root -m 0755 "${APP_ICON_FILE%/*}" "${SYMBOLIC_ICON_FILE%/*}"
install -o root -g root -m 0644 "$REPO_ROOT/assets/linux/io.nid.nid.svg" "$APP_ICON_FILE"
install -o root -g root -m 0644 "$REPO_ROOT/assets/linux/io.nid.nid-symbolic.svg" "$SYMBOLIC_ICON_FILE"
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache --quiet --force --ignore-theme-index "$ICON_THEME_DIR"
fi
if ! grep -Eqs '^[[:space:]]*allow_prelogin_input[[:space:]]*=[[:space:]]*true([[:space:]#]|$)' "$CONFIG_FILE"; then
    rm -f -- "$PRELOGIN_DROPIN"
    rmdir -- "$DROPIN_DIR" 2>/dev/null || true
fi
install -o root -g root -m 0644 "$REPO_ROOT/packaging/systemd/nidd.service" "$UNIT_FILE"
install -o root -g root -m 0644 "$REPO_ROOT/packaging/udev/70-nid.rules" "$VIRTUAL_RULE_FILE"
install -o root -g root -m 0644 "$REPO_ROOT/packaging/modules-load.d/nid.conf" "$MODULE_FILE"
install -o root -g root -m 0755 "$REPO_ROOT/packaging/system-sleep/nid" "$SLEEP_HOOK_FILE"

bash "$REPO_ROOT/packaging/linux/host-setup.sh" "$REPO_ROOT/packaging/config/nid.toml" \
    "$REPO_ROOT/packaging/udev/71-nid-capture.rules" "$VIRTUAL_RULE_FILE"
bash "$FIREWALL_SCRIPT" open

systemd-analyze verify "$UNIT_FILE"
systemctl daemon-reload
systemctl enable nidd.service
systemctl restart nidd.service

printf '\nnid is installed and nidd is running. This computer can receive input now.\n'
printf 'It also sends from every keyboard and pointer, unless capture_devices lists some.\n'
printf 'To send from only some devices:\n'
printf '  1. List input devices (* marks the ones Nidus captures):\n'
printf '     sudo %s/nid devices\n' "$BIN_DIR"
printf '  2. Select them in one command (repeat --device):\n'
printf '     sudo %s/nid setup --device /dev/input/eventX --udev-rules %s\n' \
    "$BIN_DIR" "$CAPTURE_RULE_FILE"
printf 'Check the host with: sudo %s/nid doctor\n' "$BIN_DIR"
printf 'Inspect logs with: journalctl -u nidd.service -f\n'
printf 'Remove Nidus with: sudo %s\n' "$UNINSTALLER"
printf 'Pre-login input stays disabled until you grant it during setup.\n'

printf 'GNOME users get the Nidus launcher and start the desktop agent at login.\n'
printf 'To add the GNOME extension and start the agent now, run as your desktop user (without sudo):\n'
printf '  %s/nid desktop-agent --install\n' "$BIN_DIR"
