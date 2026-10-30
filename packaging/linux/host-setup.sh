#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

readonly SERVICE_USER="nid"
readonly SERVICE_GROUP="nid"
readonly CONFIG_DIR="/etc/nid"
readonly CONFIG_FILE="$CONFIG_DIR/nid.toml"
readonly STATE_DIR="/var/lib/nid"
readonly PAIRING_WINDOW_FILE="$STATE_DIR/pairing-window"
readonly CAPTURE_RULE_FILE="/etc/udev/rules.d/71-nid-capture.rules"
readonly SAVED_CAPTURE_RULE_FILE="/etc/udev/71-nid-capture.rules.disabled"

die() {
    printf 'nid setup: %s\n' "$*" >&2
    exit 1
}

adopt_zflow() {
    local old new
    if getent group zflow >/dev/null && ! getent group "$SERVICE_GROUP" >/dev/null; then
        groupmod --new-name "$SERVICE_GROUP" zflow
    fi
    if getent passwd zflow >/dev/null && ! getent passwd "$SERVICE_USER" >/dev/null; then
        usermod --login "$SERVICE_USER" --home "$STATE_DIR" --comment "nid input daemon" zflow
    fi
    if [[ -d /etc/zflow && ! -L /etc/zflow && ! -e "$CONFIG_DIR" ]]; then
        mv -- /etc/zflow "$CONFIG_DIR"
        for old in "$CONFIG_DIR"/zflow.toml*; do
            [[ -f "$old" && ! -L "$old" ]] || continue
            mv -- "$old" "$CONFIG_DIR/nid.toml${old#"$CONFIG_DIR"/zflow.toml}"
        done
        if [[ -f "$CONFIG_FILE" && ! -L "$CONFIG_FILE" ]]; then
            sed -i -e 's|/var/lib/zflow|/var/lib/nid|g' -e 's|/run/zflow/zflowd\.sock|/run/nid/nidd.sock|g' "$CONFIG_FILE"
        fi
    fi
    if [[ -d /var/lib/zflow && ! -L /var/lib/zflow && ! -e "$STATE_DIR" ]]; then
        mv -- /var/lib/zflow "$STATE_DIR"
    fi
    for old in /etc/udev/rules.d/71-zflow-capture.rules /etc/udev/71-zflow-capture.rules.disabled; do
        new="${old//zflow/nid}"
        if [[ -f "$old" && ! -L "$old" && ! -e "$new" ]]; then
            sed -e 's/ZFLOW_/NID_/g' -e 's/zflow/nid/g' "$old" >"$new"
            rm -f -- "$old"
        fi
    done
    old=/etc/systemd/system/zflowd.service.d/prelogin.conf
    new=/etc/systemd/system/nidd.service.d/prelogin.conf
    if [[ -f "$old" && ! -L "$old" && ! -e "$new" ]]; then
        install -d -m 0755 "${new%/*}"
        mv -- "$old" "$new"
        rmdir -- "${old%/*}" 2>/dev/null || true
    fi
}

ensure_service_account() {
    local group_record members passwd_record uid gid shell uid_min nologin

    if group_record="$(getent group "$SERVICE_GROUP")"; then
        IFS=: read -r _ _ _ members <<<"$group_record"
        [[ -z "$members" ]] || die "group $SERVICE_GROUP has members; remove them before installing"
    else
        groupadd --system "$SERVICE_GROUP"
    fi

    if passwd_record="$(getent passwd "$SERVICE_USER")"; then
        IFS=: read -r _ _ uid gid _ _ shell <<<"$passwd_record"
        uid_min="$(awk '$1 == "UID_MIN" { print $2; exit }' /etc/login.defs)"
        uid_min="${uid_min:-1000}"
        [[ "$uid" =~ ^[0-9]+$ && "$uid" -lt "$uid_min" ]] || die "$SERVICE_USER exists but is not a system account"
        [[ "$gid" == "$(getent group "$SERVICE_GROUP" | cut -d: -f3)" ]] || die "$SERVICE_USER does not use group $SERVICE_GROUP"
        case "$shell" in
            */nologin|*/false) ;;
            *) die "$SERVICE_USER has a login shell: $shell" ;;
        esac
        return
    fi

    nologin="$(command -v nologin || true)"
    [[ -n "$nologin" ]] || nologin="/usr/sbin/nologin"
    [[ -x "$nologin" ]] || die "could not find a nologin executable"
    useradd \
        --system \
        --gid "$SERVICE_GROUP" \
        --home-dir "$STATE_DIR" \
        --no-create-home \
        --shell "$nologin" \
        --comment "Nidus input daemon" \
        "$SERVICE_USER"
}

[[ $# -eq 3 ]] || die "usage: host-setup.sh DEFAULT_CONFIG CAPTURE_RULES_TEMPLATE VIRTUAL_RULES"
readonly DEFAULT_CONFIG="$1" CAPTURE_TEMPLATE="$2" VIRTUAL_RULE_FILE="$3"
[[ "$EUID" -eq 0 ]] || die "run as root"
for command in awk cut getent grep groupadd groupmod install mktemp modprobe runuser sed udevadm useradd usermod; do
    command -v "$command" >/dev/null 2>&1 || die "missing required command: $command"
done

adopt_zflow
ensure_service_account

for path in "$CONFIG_DIR" "$CONFIG_FILE" "$STATE_DIR" "$PAIRING_WINDOW_FILE" "$CAPTURE_RULE_FILE" "$SAVED_CAPTURE_RULE_FILE"; do
    [[ ! -L "$path" ]] || die "refusing symlink: $path"
done

install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0700 "$CONFIG_DIR" "$STATE_DIR"
if [[ ! -e "$CONFIG_FILE" ]]; then
    eligible="$(mktemp)"
    printf 'eligible\n' >"$eligible"
    install -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0600 "$eligible" "$PAIRING_WINDOW_FILE"
    rm -f -- "$eligible"
    install -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0600 "$DEFAULT_CONFIG" "$CONFIG_FILE"
fi
[[ -f "$CONFIG_FILE" ]] || die "configuration is not a regular file: $CONFIG_FILE"
chown "$SERVICE_USER:$SERVICE_GROUP" "$CONFIG_FILE"
chmod 0600 "$CONFIG_FILE"

install -d -m 0755 /etc/udev/rules.d
if [[ ! -e "$CAPTURE_RULE_FILE" ]]; then
    if [[ -f "$SAVED_CAPTURE_RULE_FILE" ]]; then
        mv -- "$SAVED_CAPTURE_RULE_FILE" "$CAPTURE_RULE_FILE"
    else
        install -m 0644 "$CAPTURE_TEMPLATE" "$CAPTURE_RULE_FILE"
    fi
fi
[[ -f "$CAPTURE_RULE_FILE" ]] || die "capture rules are not a regular file: $CAPTURE_RULE_FILE"
chown root:root "$CAPTURE_RULE_FILE"
chmod 0644 "$CAPTURE_RULE_FILE"

if [[ -d /run/systemd/system && -S /run/udev/control ]]; then
    udevadm verify "$VIRTUAL_RULE_FILE" "$CAPTURE_RULE_FILE"
    modprobe uinput
    udevadm control --reload-rules
    udevadm trigger --action=change --subsystem-match=misc --sysname-match=uinput
    udevadm trigger --action=change --subsystem-match=input
    udevadm settle
    [[ -c /dev/uinput ]] || die "uinput loaded but /dev/uinput is missing"
    runuser -u "$SERVICE_USER" -- /usr/bin/test -w /dev/uinput \
        || die "$SERVICE_USER cannot write /dev/uinput after applying udev rules"

    shopt -s nullglob
    for event_node in /dev/input/event*; do
        if udevadm info --query=property --name="$event_node" | grep -qx 'NID_CAPTURE=1'; then
            runuser -u "$SERVICE_USER" -- /usr/bin/test -r "$event_node" \
                || die "$SERVICE_USER cannot read selected device $event_node"
        fi
    done
    shopt -u nullglob
fi
