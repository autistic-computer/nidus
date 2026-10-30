#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
    cat <<'USAGE'
Usage: bash install.sh [options]

  --version VERSION     Install a release tag (default: latest)
  --headless            Linux: install the service without GNOME integration
  --no-launch           Do not open the app after installation
  --update FROM_VERSION Linux: update this installed version without desktop setup
  --gui                 Linux update: require graphical administrator approval
  --yes                 Accepted for older command lines; changes nothing
  -h, --help            Show this help

Run as your normal user. The administrator password prompt is the only
confirmation; cancel it to stop without changing anything.
USAGE
}

die() { printf 'nid install: %s\n' "$*" >&2; exit 1; }
say() { printf '\n%s\n' "$*"; }
require() { command -v "$1" >/dev/null 2>&1 || die "Required command is missing: $1"; }

version_at_least() {
    awk -v actual="$1" -v required="$2" 'BEGIN {
        split(actual, a, "."); split(required, r, ".");
        for (i = 1; i <= 3; i++) {
            if (a[i]+0 > r[i]+0) exit 0;
            if (a[i]+0 < r[i]+0) exit 1;
        }
        exit 0;
    }'
}

as_root() {
    if [[ "${gui:-false}" == true ]]; then
        require pkexec
        pkexec --disable-internal-agent "$@" </dev/null
    elif [[ "$platform" == Linux && -n "${DBUS_SESSION_BUS_ADDRESS:-}" && -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]] && command -v pkexec >/dev/null 2>&1; then
        pkexec --disable-internal-agent "$@" </dev/null
    else
        require sudo
        sudo -- "$@" </dev/null
    fi
}

fetch() {
    curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail --silent --show-error \
        --location --retry 3 --connect-timeout 20 --max-time 600 --output "$2" "$1" </dev/null
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'
}

cleanup() {
    local status=$?
    trap - EXIT
    if [[ -n "${mac_stage:-}" ]]; then
        if [[ -d "$mac_stage/previous.app" && ! -e "$mac_destination" ]]; then
            if ! as_root mv "$mac_stage/previous.app" "$mac_destination"; then
                printf 'Previous app retained at %s/previous.app\n' "$mac_stage" >&2
                mac_stage=''
            fi
        fi
        if [[ -n "$mac_stage" ]]; then as_root rm -rf -- "$mac_stage" || true; fi
    fi
    if [[ -n "${work_dir:-}" ]]; then rm -rf -- "$work_dir"; fi
    exit "$status"
}

linux_dependencies() {
    if command -v apt-get >/dev/null 2>&1; then
        manager=apt-get
        packages=(kmod udev passwd util-linux curl)
        if [[ "$desktop" == true ]]; then packages+=(libxkbcommon-x11-0 libwayland-client0 libvulkan1 mesa-vulkan-drivers pkexec); fi
    elif command -v dnf >/dev/null 2>&1; then
        manager=dnf
        packages=(kmod systemd-udev shadow-utils util-linux curl)
        if [[ "$desktop" == true ]]; then packages+=(libxkbcommon-x11 libwayland-client vulkan-loader mesa-vulkan-drivers polkit); fi
    elif command -v pacman >/dev/null 2>&1; then
        manager=pacman
        packages=(kmod systemd shadow util-linux curl)
        if [[ "$desktop" == true ]]; then packages+=(libxkbcommon-x11 wayland vulkan-icd-loader polkit); fi
    else
        die 'Automatic dependency installation supports apt, dnf, and pacman. Install the runtime dependencies and use the installation script inside the release archive.'
    fi
}

root_install() {
    local source=$1 checksum=$2 previous=$3 installed_cli=$4 asset=${1##*/} manager
    shift 4
    update_lock=false
    if [[ -n "$previous" ]]; then
        umask 077
        exec 9>/run/nid-update.lock
        flock --exclusive 9
        update_lock=true
        [[ "$("$installed_cli" --version)" == "nid $previous" ]] \
            || die 'Nidus changed while this update was downloading. Check for updates again.'
    fi
    root_stage=$(mktemp -d /tmp/nid-install.XXXXXXXX)
    trap 'rm -rf -- "$root_stage"; if [[ "$update_lock" == true ]]; then flock --unlock 9; fi' EXIT
    cp -- "$source" "$root_stage/$asset"
    [[ "$(sha256_of "$root_stage/$asset")" == "$checksum" ]] || die "Checksum mismatch for $asset; nothing was installed."
    if [[ "$asset" == *.deb ]]; then
        chmod 0755 "$root_stage"
        chmod 0644 "$root_stage/$asset"
        apt-get update
        apt-get install -y "$@" "$root_stage/$asset"
        return
    fi
    manager=$1
    shift
    case "$manager" in
        apt-get) apt-get update; apt-get install -y "$@" ;;
        dnf) dnf install -y "$@" ;;
        pacman) pacman -S --needed --noconfirm "$@" ;;
        *) die "Unsupported package manager: $manager" ;;
    esac
    mkdir "$root_stage/payload"
    tar -xzf "$root_stage/$asset" --no-same-owner --strip-components=1 -C "$root_stage/payload"
    bash "$root_stage/payload/scripts/install.sh" --install-built
}

check_linux() {
    require udevadm
    udevadm --help | grep 'verify' >/dev/null || die 'udevadm verify is required (systemd 254 or newer).'
}

check_macos() {
    version_at_least "$(sw_vers -productVersion)" 26.0 || die 'The native app requires macOS 26 or newer.'
    require codesign
    require ditto
}

latest_version() {
    local url
    url=$(curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail --silent --show-error \
        --head --location --retry 3 --connect-timeout 20 --max-time 60 \
        --output /dev/null --write-out '%{url_effective}' \
        https://github.com/autistic-computer/nidus/releases/latest </dev/null) \
        || die 'No release could be found. Check https://github.com/autistic-computer/nidus/releases.'
    [[ "$url" == https://github.com/autistic-computer/nidus/releases/tag/* ]] || die 'Unexpected release redirect.'
    printf '%s\n' "${url##*/}"
}

verify_download() {
    checksum=$(awk -v name="$asset" '$2 == name && NF == 2 {hash=$1; count++} END {if (count == 1) print hash; else exit 1}' "$work_dir/SHA256SUMS") \
        || die "The release has no unique checksum for $asset."
    [[ "$checksum" =~ ^[0-9a-f]{64}$ ]] || die 'Invalid release checksum.'
    [[ "$(sha256_of "$work_dir/$asset")" == "$checksum" ]] || die "Checksum mismatch for $asset; nothing was installed."
}

archive_install_present() {
    [[ -e /usr/local/bin/nid || -e /usr/local/bin/nidd || -e /etc/systemd/system/nidd.service || -e /usr/local/lib/zflow/uninstall.sh ]]
}

select_artifact() {
    local machine libc_version
    machine=$(uname -m)
    if [[ "$platform" == Darwin && "$machine" == x86_64 ]] && [[ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" == 1 ]]; then
        machine=arm64
    fi
    case "$machine" in
        x86_64|amd64) architecture=x86_64 ;;
        arm64|aarch64) architecture=aarch64 ;;
        *) die "No release binary is available for $machine." ;;
    esac
    use_deb=false
    if [[ "$platform" == Linux ]]; then
        libc_version=$(getconf GNU_LIBC_VERSION 2>/dev/null) || die 'Linux release binaries require glibc 2.39 or newer; musl is not supported.'
        version_at_least "${libc_version#glibc }" 2.39 || die 'Linux release binaries require glibc 2.39 or newer.'
        if command -v apt-get >/dev/null 2>&1 && ! archive_install_present; then
            use_deb=true
        elif command -v dpkg-query >/dev/null 2>&1 && { dpkg-query -W -f='${Status}\n' nid zflow 2>/dev/null || true; } | grep -qx 'install ok installed'; then
            use_deb=true
        fi
        if [[ "$use_deb" == true ]]; then
            deb_arch=$(dpkg --print-architecture)
            case "$deb_arch" in
                amd64|arm64) asset="nid_${version#v}_${deb_arch}.deb" ;;
                *) die "No release package is available for $deb_arch." ;;
            esac
        else
            asset="nid-${version}-${architecture}-unknown-linux-gnu.tar.gz"
        fi
    else
        asset="nid-${version}-universal-apple-darwin.tar.gz"
        legacy_asset="nid-${version}-${architecture}-apple-darwin.tar.gz"
    fi
}

describe_linux() {
    if [[ "$use_deb" == true ]]; then
        printf '  - the nid command and the nidd service, from %s and apt\n' "$asset"
    else
        linux_dependencies
        printf '  - the nid command and the nidd service, under /usr/local\n'
        printf '  - runtime packages from %s: %s\n' "$manager" "${packages[*]}"
    fi
    printf '  - a locked nid system account that types remote input through /dev/uinput\n'
    printf '  - an allow rule for UDP port 43119 when ufw or firewalld is on\n'
    if [[ "$desktop" == true ]]; then
        printf '  - for every GNOME user: the Nidus launcher, and the desktop agent at login\n'
        printf '  - for you: the Nidus GNOME extension (GNOME asks before it downloads it)\n'
    fi
}

install_linux() {
    say 'Installing the service. An existing Nidus connection will disconnect during its restart.'
    if [[ "$use_deb" == true ]]; then installed_cli=/usr/bin/nid; else installed_cli=/usr/local/bin/nid; fi
    local root_args=("$work_dir/$asset" "$checksum" "${update_from:-}" "$installed_cli")
    if [[ "$use_deb" == true ]]; then
        if [[ "$desktop" == true ]]; then root_args+=(--install-recommends); else root_args+=(--no-install-recommends); fi
    else
        linux_dependencies
        root_args+=("$manager" "${packages[@]}")
    fi
    as_root bash -c "set -euo pipefail; $(declare -f die sha256_of root_install); root_install \"\$@\"" \
        nid-install "${root_args[@]}"
    if [[ "${update:-false}" == true ]]; then return; fi
    if [[ "$desktop" == false ]]; then
        say 'The Linux service is installed. Run nid desktop-agent --install from a GNOME session to add the desktop app.'
        return
    fi
    say 'Setting up GNOME for your account…'
    "$installed_cli" desktop-agent --install </dev/null \
        || die 'The service is installed, but desktop setup failed. Run nid desktop-agent --install after correcting the error above.'
    if [[ "$launch" == true ]]; then
        "$installed_cli" settings </dev/null >/dev/null 2>&1 &
    fi
}

quit_mac_app() {
    local id=$1 process=$2 attempt
    pgrep -x "$process" >/dev/null || return 0
    say 'Closing Nidus before updating the app…'
    osascript -e "tell application id \"$id\" to quit" </dev/null
    for ((attempt = 0; attempt < 20; attempt++)); do
        if ! pgrep -x "$process" >/dev/null; then return 0; fi
        sleep 0.5
    done
    die 'Nidus is still running. Quit it and rerun the installer.'
}

install_macos() {
    local bundle="$payload_dir/Nidus.app"
    [[ -d "$bundle" && ! -L "$bundle" ]] || die 'The release archive does not contain Nidus.app.'
    codesign --verify --strict --deep "$bundle"
    [[ ! -L "$mac_destination" ]] || die "Refusing to replace a symlink at $mac_destination."
    if [[ -e "$mac_destination" ]]; then
        [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$mac_destination/Contents/Info.plist")" == io.nid.nid ]] \
            || die "$mac_destination is not a Nidus app bundle."
    fi
    local zflow_app=''
    if [[ -d /Applications/zflow.app && ! -L /Applications/zflow.app ]] \
        && [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' /Applications/zflow.app/Contents/Info.plist 2>/dev/null)" == io.zflow.zflow ]]; then
        zflow_app=/Applications/zflow.app
        quit_mac_app io.zflow.zflow zflow-app
    fi
    quit_mac_app io.nid.nid nid-app
    say "Installing ${mac_destination}…"
    mac_stage=$(as_root mktemp -d "${mac_destination%/*}/.nid-install.XXXXXXXX")
    as_root ditto "$bundle" "$mac_stage/Nidus.app"
    as_root codesign --verify --strict --deep "$mac_stage/Nidus.app"
    if [[ -e "$mac_destination" ]]; then as_root mv "$mac_destination" "$mac_stage/previous.app"; fi
    as_root mv "$mac_stage/Nidus.app" "$mac_destination"
    as_root rm -rf -- "$mac_stage"
    mac_stage=''
    if [[ -n "$zflow_app" ]]; then as_root rm -rf -- "$zflow_app"; fi
    say 'Installed Nidus. Allow Accessibility and Local Network access when the app requests them.'
    if codesign --display --verbose=2 "$mac_destination" 2>&1 | grep 'Signature=adhoc' >/dev/null; then
        printf 'This build uses an ad-hoc signature. Input sharing works; AWDL helper installation requires an Apple signing identity.\n'
    fi
    if [[ "$launch" == true ]]; then open "$mac_destination"; fi
}

main() {
    local headless=false command base_url
    platform=$(uname -s)
    version=latest
    launch=true
    desktop=false
    update=false
    update_from=''
    gui=false
    work_dir=''
    mac_stage=''
    mac_destination=/Applications/Nidus.app
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --yes) ;;
            --headless) headless=true ;;
            --no-launch) launch=false ;;
            --update)
                [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || die '--update requires the installed version'
                update=true; update_from="$2"; shift ;;
            --gui) gui=true ;;
            --version)
                [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || die '--version requires a release version'
                version="$2"; shift ;;
            -h|--help) usage; return ;;
            *) die "Unknown option: $1" ;;
        esac
        shift
    done
    case "$platform" in Linux|Darwin) ;; *) die "Unsupported operating system: $platform.";; esac
    [[ "$(id -u)" != 0 ]] || die 'Run this installer as your normal user, without sudo. It requests administrator access when needed.'
    [[ -n "${HOME:-}" && "$HOME" == /* ]] || die 'HOME must be an absolute path.'
    [[ "$platform" == Linux || "$headless" == false ]] || die '--headless is only available on Linux.'
    [[ "$platform" == Linux || "$update" == false && "$gui" == false ]] || die '--update and --gui are only available on Linux.'
    if [[ "$gui" == true ]]; then
        [[ "$update" == true ]] || die '--gui requires --update.'
        [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] || die 'Open updates from your desktop session.'
        require pkexec
    fi
    for command in curl tar awk mktemp; do require "$command"; done
    if ! command -v sha256sum >/dev/null 2>&1; then require shasum; fi
    if [[ "$version" == latest ]]; then version=$(latest_version); fi
    case "$version" in v*) ;; *) version="v$version";; esac
    [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || die 'Invalid release version.'
    if [[ "$platform" == Linux ]]; then
        if [[ "$update" == true ]]; then require flock; fi
        require systemctl
        [[ -n "$(systemctl show --property=Version --value 2>/dev/null)" ]] || die 'Linux installation requires a running systemd system manager.'
        if [[ "$headless" == false && -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
            case ":${XDG_CURRENT_DESKTOP:-}:" in *:GNOME:*|*:gnome:*) desktop=true;; esac
        fi
        check_linux
    else
        check_macos
    fi
    select_artifact
    say "Nidus binary installer ($version, $platform $architecture)"
    printf 'Download: %s\n' "$asset"
    if [[ "$platform" == Linux ]]; then
        printf 'This installs:\n'
        describe_linux
    fi
    printf 'Existing service/app configuration and paired identities are preserved.\n'
    printf 'The administrator password prompt confirms the installation.\n'
    work_dir=$(mktemp -d "${TMPDIR:-/tmp}/nid-install.XXXXXXXX")
    trap cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    base_url="https://github.com/autistic-computer/nidus/releases/download/$version"
    fetch "$base_url/SHA256SUMS" "$work_dir/SHA256SUMS" || die "Checksums are unavailable for $version."
    if [[ "$platform" == Darwin ]] && ! awk -v name="$asset" '$2 == name {found = 1} END {exit !found}' "$work_dir/SHA256SUMS"; then
        asset=$legacy_asset
        printf 'This release has no universal archive. Download: %s\n' "$asset"
    fi
    fetch "$base_url/$asset" "$work_dir/$asset" || die "No downloadable artifact for $platform $architecture in $version."
    verify_download
    if [[ "$use_deb" == false ]]; then
        payload_dir="$work_dir/payload"
        mkdir "$payload_dir"
        tar -xzf "$work_dir/$asset" --strip-components=1 -C "$payload_dir"
        if [[ "$platform" == Linux ]]; then
            [[ -x "$payload_dir/bin/nid" && -x "$payload_dir/bin/nidd" && -f "$payload_dir/scripts/install.sh" ]] \
                || die 'The release archive is missing the Linux binaries or installer.'
        fi
    fi
    if [[ "$platform" == Linux ]]; then install_linux; else install_macos; fi
    say 'Nidus installation finished.'
}

if [[ -z "${BASH_SOURCE[0]:-}" || "${BASH_SOURCE[0]:-}" == "$0" ]]; then main "$@"; fi
