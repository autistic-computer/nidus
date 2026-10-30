#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

readonly SERVICE_USER="nid"
readonly SERVICE_GROUP="nid"
readonly CONFIG_DIR="/etc/nid"
readonly STATE_DIR="/var/lib/nid"
readonly UNIT_FILE="/etc/systemd/system/nidd.service"
readonly DROPIN_DIR="/etc/systemd/system/nidd.service.d"
readonly PRELOGIN_DROPIN="$DROPIN_DIR/prelogin.conf"
readonly VIRTUAL_RULE_FILE="/etc/udev/rules.d/70-nid.rules"
readonly CAPTURE_RULE_FILE="/etc/udev/rules.d/71-nid-capture.rules"
readonly MODULE_FILE="/etc/modules-load.d/nid.conf"
readonly SLEEP_HOOK_FILE="/usr/lib/systemd/system-sleep/nid"
readonly SLEEP_MARKER="/run/nidd-resume-after-sleep"
readonly NID_BIN="/usr/local/bin/nid"
readonly NIDD_BIN="/usr/local/bin/nidd"
readonly NID_APP_BIN="/usr/local/bin/nid-app"
readonly LIB_DIR="/usr/local/lib/nid"
readonly NID_GUI_BIN="/usr/local/bin/nid-gui"
readonly FIREWALL_SCRIPT="$LIB_DIR/firewall.sh"
readonly DESKTOP_FILE="/usr/local/share/applications/io.nid.nid.desktop"
readonly DBUS_SERVICE_DIR="/usr/local/share/dbus-1/services"
readonly DBUS_SERVICE_FILE="$DBUS_SERVICE_DIR/io.nid.Desktop.service"
readonly AUTOSTART_FILE="/etc/xdg/autostart/io.nid.desktop-agent.desktop"
readonly ICON_THEME_DIR="/usr/local/share/icons/hicolor"
readonly APP_ICON_FILE="$ICON_THEME_DIR/scalable/apps/io.nid.nid.svg"
readonly SYMBOLIC_ICON_FILE="$ICON_THEME_DIR/symbolic/apps/io.nid.nid-symbolic.svg"

purge=false

die() {
    printf 'uninstall: %s\n' "$*" >&2
    exit 1
}

remove_tree() {
    local path="$1"
    case "$path" in
        /etc/nid|/var/lib/nid) ;;
        *) die "refusing to purge unexpected path: $path" ;;
    esac
    [[ ! -e "$path" && ! -L "$path" ]] || find "$path" -xdev -depth -delete
}

case "${1:-}" in
    "") ;;
    --purge) purge=true ;;
    -h|--help)
        printf 'usage: sudo %s [--purge]\n' "$0"
        printf 'Without --purge, configuration, identity state, and the service account remain.\n'
        exit 0
        ;;
    *) die "unknown argument: $1" ;;
esac
[[ $# -le 1 ]] || die "too many arguments"

[[ "$(uname -s)" == "Linux" ]] || die "Linux is required"
if command -v dpkg-query >/dev/null 2>&1 && [[ "$(dpkg-query -W -f='${Status}' nid 2>/dev/null || true)" == 'install ok installed' ]]; then
    die 'Nidus is managed by dpkg. Remove it with: sudo apt remove nid'
fi
[[ "$EUID" -eq 0 ]] || die "run as root: sudo $0"

if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-active --quiet nidd.service; then
        systemctl stop nidd.service || die "could not stop nidd.service"
    fi
    systemctl disable nidd.service >/dev/null 2>&1 || true
fi
if [[ -f "$FIREWALL_SCRIPT" ]]; then
    bash "$FIREWALL_SCRIPT" close
fi

rm -f -- \
    "$UNIT_FILE" \
    "$PRELOGIN_DROPIN" \
    "$VIRTUAL_RULE_FILE" \
    "$CAPTURE_RULE_FILE" \
    "$MODULE_FILE" \
    "$SLEEP_HOOK_FILE" \
    "$SLEEP_MARKER" \
    "$NID_BIN" \
    "$NIDD_BIN" \
    "$NID_APP_BIN" \
    "$LIB_DIR/uninstall.sh" \
    "$FIREWALL_SCRIPT" \
    "$NID_GUI_BIN" \
    "$DESKTOP_FILE" \
    "$DBUS_SERVICE_FILE" \
    "$AUTOSTART_FILE" \
    "$APP_ICON_FILE" \
    "$SYMBOLIC_ICON_FILE"
if [[ -d "$ICON_THEME_DIR" ]] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache --quiet --force --ignore-theme-index "$ICON_THEME_DIR"
fi
rmdir -- "$LIB_DIR" "$DROPIN_DIR" 2>/dev/null || true
rmdir -- "$DBUS_SERVICE_DIR" "${DBUS_SERVICE_DIR%/*}" 2>/dev/null || true

if [[ "$purge" == true ]]; then
    for command in find getent groupdel userdel; do
        command -v "$command" >/dev/null 2>&1 || die "missing required command: $command"
    done
    remove_tree "$CONFIG_DIR"
    remove_tree "$STATE_DIR"
    if getent passwd "$SERVICE_USER" >/dev/null 2>&1; then
        userdel "$SERVICE_USER"
    fi
    if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        groupdel "$SERVICE_GROUP"
    fi
fi

if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload
    systemctl reset-failed nidd.service >/dev/null 2>&1 || true
fi
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules
    udevadm trigger --action=change --subsystem-match=misc --sysname-match=uinput || true
    udevadm trigger --action=change --subsystem-match=input || true
    udevadm settle || true
fi

if [[ "$purge" == true ]]; then
    printf 'Removed Nidus binaries, service, rules, configuration, state, and service account.\n'
else
    printf 'Removed Nidus binaries, udev rules, service, and desktop launcher.\n'
    printf 'Preserved %s, %s, and the %s account.\n' \
        "$CONFIG_DIR" "$STATE_DIR" "$SERVICE_USER"
    printf 'Removed capture rules so the account no longer gains input-device access.\n'
    printf 'Run this script with --purge to delete the preserved data.\n'
fi
printf 'Desktop users keep the Nidus GNOME extension until they remove it in Extensions.\n'
