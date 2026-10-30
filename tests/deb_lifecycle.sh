#!/usr/bin/env bash
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
architecture=$(dpkg --print-architecture)
packages=("$root/target/dist/"nid_*_"$architecture.deb")
[[ ${#packages[@]} == 1 && -f "${packages[0]}" ]] || { echo 'Build one native .deb first.' >&2; exit 1; }
docker run --rm -i --mount "type=bind,src=${packages[0]},dst=/tmp/nid.deb,readonly" ubuntu:24.04 bash -es <<'CONTAINER'
set -o pipefail
export DEBIAN_FRONTEND=noninteractive
dpkg-deb -e /tmp/nid.deb /tmp/control
grep -q '_dh_action=restart' /tmp/control/postinst
# Stand-ins for an active ufw and a running firewalld that log what the package asks
# for. apt runs maintainer scripts without /usr/local on PATH, so they go where the
# real tools live.
cat > /usr/sbin/ufw <<'FAKE'
#!/bin/sh
echo "$*" >> /tmp/ufw.log
if [ "$1" = status ]; then echo 'Status: active'; fi
FAKE
cat > /usr/bin/firewall-cmd <<'FAKE'
#!/bin/sh
echo "$*" >> /tmp/firewalld.log
case "$*" in
    *--get-default-zone*) echo public ;;
    *--get-zones*) echo 'public home' ;;
    *--zone=public*--query-service=nid*) grep -q -- '--permanent --add-service=nid' /tmp/firewalld.log ;;
    *--query-service=nid*) exit 1 ;;
esac
FAKE
chmod 0755 /usr/sbin/ufw /usr/bin/firewall-cmd
apt-get update -qq
apt-get install -y -qq --no-install-recommends /tmp/nid.deb | tee /tmp/install.log
test -x /usr/bin/nid
/usr/bin/nid --version
# The settings window. Its linked libraries come from the package's dependencies.
test -x /usr/bin/nid-app
if ldd /usr/bin/nid-app | grep 'not found'; then echo 'nid-app is missing libraries' >&2; exit 1; fi
# The extension is what scripts/pack-extension.sh uploads, plus Shell's code again in the build
# folder `current` names, which the loader reloads without a new login.
extension=/usr/share/gnome-shell/extensions/nid@autistic.computer
test "$(ls "$extension" | grep -v '^build-' | tr '\n' ' ')" = 'client.js current extension.js indicator.js metadata.json shell.js '
build=$(cat "$extension/current")
test "$(ls "$extension/$build" | tr '\n' ' ')" = 'client.js indicator.js shell.js '
cmp "$extension/shell.js" "$extension/$build/shell.js"
test -f /usr/share/applications/io.nid.nid.desktop
grep -qx 'Icon=io.nid.nid' /usr/share/applications/io.nid.nid.desktop
test -f /usr/share/icons/hicolor/scalable/apps/io.nid.nid.svg
test -f /usr/share/icons/hicolor/symbolic/apps/io.nid.nid-symbolic.svg
# Every GNOME user starts the agent at login and can activate it over D-Bus.
grep -qx 'Exec=/usr/bin/nid desktop-agent' /etc/xdg/autostart/io.nid.desktop-agent.desktop
grep -qx 'Exec=/usr/bin/nid desktop-agent' /usr/share/dbus-1/services/io.nid.Desktop.service
test -f /usr/lib/systemd/system/nidd.service
# Pre-login boot ordering is opt-in through nid setup --prelogin on.
test ! -e /usr/lib/systemd/system/nidd.service.d/prelogin.conf
grep -qx 'ExecStart=/usr/bin/nidd --config /etc/nid/nid.toml' /usr/lib/systemd/system/nidd.service
test "$(stat -c '%U:%G:%a' /etc/nid/nid.toml)" = nid:nid:600
test "$(stat -c '%U:%G:%a' /var/lib/nid)" = nid:nid:700
test "$(getent group nid | cut -d: -f4)" = ''
# Both firewalls get the one UDP port, and the installer says so.
grep -qx 'allow nid' /tmp/ufw.log
grep -qx 'ports=43119/udp' /etc/ufw/applications.d/nid
grep -q 'Opened UDP port 43119 in ufw' /tmp/install.log
grep -qx -- '--permanent --add-service=nid' /tmp/firewalld.log
grep -qx -- '--add-service=nid' /tmp/firewalld.log
grep -q 'port protocol="udp" port="43119"' /etc/firewalld/services/nid.xml
test "$(grep -c 43120 /etc/firewalld/services/nid.xml)" = 0
grep -q 'in firewalld (service nid, zone public)' /tmp/install.log
printf '\n# Preserve this edit\n' >> /etc/nid/nid.toml
printf '# Preserve selected devices\n' > /etc/udev/rules.d/71-nid-capture.rules
printf 'identity sentinel\n' > /var/lib/nid/identity-test
cp /etc/nid/nid.toml /tmp/expected.toml
cp /etc/udev/rules.d/71-nid-capture.rules /tmp/expected.rules
dpkg -i /tmp/nid.deb
cmp /etc/nid/nid.toml /tmp/expected.toml
cmp /etc/udev/rules.d/71-nid-capture.rules /tmp/expected.rules
mkdir -p /etc/systemd/system/nidd.service.d
printf '[Unit]\nBefore=display-manager.service\n' > /etc/systemd/system/nidd.service.d/prelogin.conf
dpkg --remove nid
test -f /etc/systemd/system/nidd.service.d/prelogin.conf
test ! -e /usr/bin/nid
test ! -e /usr/bin/nid-app
test ! -e /usr/share/icons/hicolor/scalable/apps/io.nid.nid.svg
test ! -e /usr/share/icons/hicolor/symbolic/apps/io.nid.nid-symbolic.svg
test ! -e /usr/share/dbus-1/services/io.nid.Desktop.service
# dpkg keeps the autostart conffile until purge; TryExec keeps it from running.
grep -qx 'TryExec=/usr/bin/nid' /etc/xdg/autostart/io.nid.desktop-agent.desktop
# Removal closes the ports again.
grep -qx 'delete allow nid' /tmp/ufw.log
test ! -e /etc/ufw/applications.d/nid
grep -qx -- '--permanent --zone=public --remove-service=nid' /tmp/firewalld.log
test ! -e /etc/firewalld/services/nid.xml
test ! -e /etc/udev/rules.d/71-nid-capture.rules
cmp /etc/udev/71-nid-capture.rules.disabled /tmp/expected.rules
test "$(stat -c '%U:%G' /etc/udev/71-nid-capture.rules.disabled)" = root:root
if runuser -u nid -- test -w /etc/udev; then echo 'Daemon can replace saved rules' >&2; exit 1; fi
cmp /etc/nid/nid.toml /tmp/expected.toml
dpkg -i /tmp/nid.deb
cmp /etc/udev/rules.d/71-nid-capture.rules /tmp/expected.rules
cmp /etc/nid/nid.toml /tmp/expected.toml
test -f /etc/xdg/autostart/io.nid.desktop-agent.desktop
dpkg --purge nid
test ! -e /etc/nid/nid.toml
test ! -e /etc/systemd/system/nidd.service.d
test ! -e /etc/udev/rules.d/71-nid-capture.rules
test ! -e /etc/xdg/autostart/io.nid.desktop-agent.desktop
grep -qx 'identity sentinel' /var/lib/nid/identity-test
# A package must not silently shadow a previous /usr/local installation.
mkdir -p /usr/local/bin
touch /usr/local/bin/nid
if dpkg -i /tmp/nid.deb; then echo 'Legacy installation was not rejected' >&2; exit 1; fi
test ! -e /usr/bin/nid
echo 'Debian install, firewall, session files, upgrade, removal, reinstall, purge, and legacy detection passed.'
CONTAINER
