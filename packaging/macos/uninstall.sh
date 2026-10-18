#!/bin/sh
# LocalWeb macOS uninstaller (draft).
#
# Mirrors the Debian maintainer scripts (packaging/maintainer/prerm and
# postrm): stop everything LocalWeb owns, drop its system integration, and
# wipe its data. Project source directories are never touched.
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

home_for_uid() {
    name=$(dscl . -list /Users UniqueID 2>/dev/null | awk -v uid="$1" '$2 == uid { print $1; exit }')
    [ -n "$name" ] || return 1
    dscl . -read "/Users/$name" NFSHomeDirectory 2>/dev/null | awk '{ print $2 }'
}

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

# 3. Sweep the per-user paths registered by the privileged helper. The guard
#    mirrors the Debian postrm: only paths under the user's home or the
#    per-user temp directory are removed, never anything else.
if [ -d "$VARDIR/users" ]; then
    for registry in "$VARDIR/users"/*.paths; do
        [ -f "$registry" ] || continue
        uid=${registry##*/}
        uid=${uid%.paths}
        home=$(home_for_uid "$uid") || continue
        [ -n "$home" ] || continue
        while IFS= read -r path; do
            case "$path" in
                "$home"/*|/private/var/folders/*/*/T/localweb|/var/folders/*/*/T/localweb)
                    rm -rf -- "$path" ;;
            esac
        done < "$registry"
    done
fi

# 4. Remove per-user application state.
for home in /Users/*; do
    [ -d "$home" ] || continue
    user=${home##*/}
    [ "$user" = Shared ] && continue
    rm -rf -- \
        "$home/.config/localweb" \
        "$home/.local/share/localweb" \
        "$home/.local/state/localweb" \
        "$home/Library/Application Support/localweb" \
        "$home/Library/Logs/localweb" \
        "$home/Library/Caches/localweb"
    rm -f -- "$home/Library/LaunchAgents/dev.localweb."*.plist
done

# 5. Drop the managed /etc/hosts block, preserving everything else.
if [ -f /etc/hosts ]; then
    awk '
        /^# BEGIN LOCALWEB - DO NOT EDIT$/ { skip=1; next }
        /^# END LOCALWEB$/ { skip=0; next }
        !skip { print }
    ' /etc/hosts > /etc/hosts.localweb-tmp &&
        cat /etc/hosts.localweb-tmp > /etc/hosts &&
        rm -f /etc/hosts.localweb-tmp
fi

# 6. Drop the LocalWeb CA from the system keychain and remove its trust copy.
security delete-certificate -c "$CA_COMMON_NAME" /Library/Keychains/System.keychain 2>/dev/null || true
rm -rf -- "/Library/Application Support/LocalWeb"

# 7. Remove installed files, data and receipts.
rm -f -- "$PREFIX/bin/localweb" "$PREFIX/bin/localweb-uninstall"
rm -rf -- "$LIBEXEC" "$VARDIR" "$LOGDIR" /Applications/LocalWeb.app
pkgutil --forget dev.localweb.pkg 2>/dev/null || true

# 8. Flush the DNS cache so removed hostnames stop resolving immediately.
dscacheutil -flushcache 2>/dev/null || true
killall -HUP mDNSResponder 2>/dev/null || true

echo "LocalWeb removed. Project source directories were not touched."
