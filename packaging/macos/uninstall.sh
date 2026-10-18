#!/bin/sh
# LocalWeb macOS uninstaller (draft).
#
# Stops everything LocalWeb owns and drops its system integration. Per-user
# data (projects, databases, configurations and private CA keys) is never
# deleted automatically: package removal is not consent to destroy it.
# Use `localweb data purge` from each account instead. Project source
# directories are never touched.
#
# Run as root:  sudo localweb-uninstall
#
# NOT YET VALIDATED ON APPLE HARDWARE. See docs/macos-porting.md for the
# parity checklist that must pass before this script is shipped.

set -eu

[ "$(id -u)" = 0 ] || {
    echo "localweb-uninstall must run as root: sudo localweb-uninstall" >&2
    exit 1
}

PREFIX=/usr/local
LIBEXEC=$PREFIX/libexec/localweb
VARDIR=$PREFIX/var/lib/localweb
LOGDIR=$PREFIX/var/log/localweb
CA_COMMON_NAME="LocalWeb Development CA"

# 1. Stop and remove launchd jobs. The gateway and daemon are per-user
#    LaunchAgents; the system daemon paths are swept for older drafts.
launchctl bootout system/dev.localweb.gateway 2>/dev/null || true
rm -f /Library/LaunchDaemons/dev.localweb.*.plist

for home in /Users/*; do
    [ -d "$home" ] || continue
    user=${home##*/}
    [ "$user" = Shared ] && continue
    uid=$(id -u "$user" 2>/dev/null || true)
    [ -n "$uid" ] || continue
    launchctl bootout "gui/$uid/dev.localweb.daemon" 2>/dev/null || true
    launchctl bootout "gui/$uid/dev.localweb.gateway" 2>/dev/null || true
    rm -f "$home/Library/LaunchAgents/dev.localweb."*.plist
done

# 2. Kill any process still running from the install prefix.
pkill -f "^$LIBEXEC/" 2>/dev/null || true

# 3. Remove only the per-user launchd integration. Application state and any
#    private CA material stay in place until the user explicitly runs
#    `localweb data purge`.
for home in /Users/*; do
    [ -d "$home" ] || continue
    user=${home##*/}
    [ "$user" = Shared ] && continue
    rm -f -- "$home/Library/LaunchAgents/dev.localweb."*.plist
done

# 4. Drop the managed /etc/hosts block, preserving everything else.
if [ -f /etc/hosts ]; then
    awk '
        /^# BEGIN LOCALWEB - DO NOT EDIT$/ { skip=1; next }
        /^# END LOCALWEB$/ { skip=0; next }
        !skip { print }
    ' /etc/hosts > /etc/hosts.localweb-tmp &&
        cat /etc/hosts.localweb-tmp > /etc/hosts &&
        rm -f /etc/hosts.localweb-tmp
fi

# 5. Drop the LocalWeb CA from the system keychain and remove its trust copy.
security delete-certificate -c "$CA_COMMON_NAME" /Library/Keychains/System.keychain 2>/dev/null || true
rm -rf -- "/Library/Application Support/LocalWeb"

# 6. Remove installed files, system state and receipts.
rm -f -- "$PREFIX/bin/localweb" "$PREFIX/bin/localweb-uninstall"
rm -rf -- "$LIBEXEC" "$VARDIR" "$LOGDIR" /Applications/LocalWeb.app
pkgutil --forget dev.localweb.pkg 2>/dev/null || true

# 7. Flush the DNS cache so removed hostnames stop resolving immediately.
dscacheutil -flushcache 2>/dev/null || true
killall -HUP mDNSResponder 2>/dev/null || true

echo "LocalWeb removed. Per-user data and project source directories were preserved."
