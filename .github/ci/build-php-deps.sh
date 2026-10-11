#!/bin/sh
# Compatibility build inputs for legacy PHP APIs; no system installation.
set -eu
root="$PWD/packaging/ci-inputs/php-deps"
mkdir -p "$root"
verify() {
    python3 - "$1" "$2" <<'PY'
import hashlib, pathlib, sys
assert hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest() == sys.argv[2], 'Dependency source checksum mismatch'
PY
}
if [ ! -f "$root/libxml/lib/pkgconfig/libxml-2.0.pc" ]; then
    curl --retry 5 --retry-all-errors --retry-delay 2 -fLsS -o "$root/libxml.tar.xz" https://download.gnome.org/sources/libxml2/2.9/libxml2-2.9.14.tar.xz
    verify "$root/libxml.tar.xz" 60d74a257d1ccec0475e749cba2f21559e48139efba6ff28224357c7c798dfee
    mkdir -p "$root/xml-source"
    tar -xJf "$root/libxml.tar.xz" --strip-components=1 -C "$root/xml-source"
    (
        cd "$root/xml-source"
        CFLAGS="-O2 -fPIC -Wno-error=incompatible-pointer-types" ./configure --prefix="$root/libxml" --without-python --without-lzma --without-icu
        make -j "${LOCALWEB_BUILD_JOBS:-2}"
        make install
    )
fi
if [ ! -f "$root/openssl/lib/pkgconfig/openssl.pc" ]; then
    curl --retry 5 --retry-all-errors --retry-delay 2 -fLsS -o "$root/openssl.tar.gz" https://www.openssl.org/source/old/1.1.1/openssl-1.1.1w.tar.gz
    verify "$root/openssl.tar.gz" cf3098950cb4d853ad95c0841f1f9c6d3dc102dccfcacd521d93925208b76ac8
    mkdir -p "$root/ssl-source"
    tar -xzf "$root/openssl.tar.gz" --strip-components=1 -C "$root/ssl-source"
    (
        cd "$root/ssl-source"
        ./config --prefix="$root/openssl" --openssldir=/etc/ssl shared no-tests
        make -j "${LOCALWEB_BUILD_JOBS:-2}"
        make install_sw
    )
fi
