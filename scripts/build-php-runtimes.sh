#!/bin/sh
# Build PHP CLI/FPM pairs from checksum-verified official release sources.
# PHP 8.0 and 8.1 remain available for legacy compatibility only; both are EOL.
set -eu
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${LOCALWEB_PHP_ROOT:-$workspace/packaging/php-builds}
mkdir -p "$output"
sh "$workspace/.github/ci/build-php-deps.sh"
compat="$workspace/packaging/ci-inputs/php-deps"
while read -r version release digest; do
    [ -n "$version" ] || continue
    case "$version" in \#*) continue ;; esac
    prefix="$output/$version"
    if [ -f "$prefix/SOURCE" ] && [ -x "$prefix/sbin/php-fpm" ] && "$prefix/bin/php" -n -r "exit(PHP_VERSION === '$release' ? 0 : 1);"; then
        continue
    fi
    source_dir="$output/source-$version"
    archive="$output/php-$release.tar.xz"
    url="https://www.php.net/distributions/php-$release.tar.xz"
    attempt=1
    while :; do
        if curl --connect-timeout 30 --retry 3 --retry-all-errors --retry-delay 2 \
            -fsSL -o "$archive" "$url"; then
            break
        fi
        if [ "$attempt" -ge 8 ]; then
            printf 'error: cannot download %s after %d attempts; check runner DNS/network\n' \
                "$url" "$attempt" >&2
            exit 1
        fi
        delay=$((attempt * 5))
        printf 'warning: download of %s failed (attempt %d), retrying in %ds\n' \
            "$url" "$attempt" "$delay" >&2
        sleep "$delay"
        attempt=$((attempt + 1))
    done
    python3 - "$archive" "$digest" <<'PY'
import hashlib, pathlib, sys
assert hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest() == sys.argv[2], 'PHP source checksum mismatch'
PY
    mkdir -p "$source_dir"
    tar -xJf "$archive" --strip-components=1 -C "$source_dir"
    log_dir="$output/logs"
    mkdir -p "$log_dir"
    (
        cd "$source_dir"
        if [ -f Makefile ]; then make clean; fi
        export PKG_CONFIG_PATH="$compat/libxml/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
        if [ "$version" = 8.0 ]; then
            export PKG_CONFIG_PATH="$compat/openssl/lib/pkgconfig:$PKG_CONFIG_PATH"
        fi
        export LD_LIBRARY_PATH="$compat/libxml/lib:$compat/openssl/lib:${LD_LIBRARY_PATH:-}"
        export DYLD_LIBRARY_PATH="$compat/libxml/lib:$compat/openssl/lib:${DYLD_LIBRARY_PATH:-}"
        if ! CFLAGS="${CFLAGS:--O2 -std=gnu17}" CXXFLAGS="${CXXFLAGS:--std=c++17}" ./configure \
            --prefix="$prefix" --enable-fpm --disable-cgi --without-pear \
            --enable-mbstring --enable-intl --enable-bcmath --enable-soap \
            --enable-sockets --enable-exif --enable-opcache --enable-pcntl \
            --with-curl --with-openssl --with-iconv --with-zlib --with-zip \
            --with-pdo-mysql=mysqlnd --with-mysqli=mysqlnd \
            --with-pgsql --with-pdo-pgsql --with-pdo-sqlite --with-sqlite3 \
            --enable-gd --with-jpeg --with-freetype \
            > "$log_dir/$version-configure.log" 2>&1; then
            echo "PHP $version configure failed; log: $log_dir/$version-configure.log" >&2
            tail -n 40 "$log_dir/$version-configure.log" >&2
            exit 1
        fi
        # Older intl configure rules append C++11 after CXXFLAGS. Current
        # ICU headers require C++17, so normalize the generated compile rules.
        python3 - <<'PY'
from pathlib import Path
path = Path('Makefile')
path.write_text(path.read_text().replace('-std=c++11', '-std=c++17'))
PY
        if ! make -j "${LOCALWEB_BUILD_JOBS:-2}" > "$log_dir/$version-make.log" 2>&1; then
            echo "PHP $version make failed; log: $log_dir/$version-make.log" >&2
            grep -n -m 20 -i 'error:' "$log_dir/$version-make.log" >&2 || true
            tail -n 60 "$log_dir/$version-make.log" >&2
            exit 1
        fi
        if ! make install > "$log_dir/$version-install.log" 2>&1; then
            echo "PHP $version make install failed; log: $log_dir/$version-install.log" >&2
            tail -n 40 "$log_dir/$version-install.log" >&2
            exit 1
        fi
    )
    printf 'https://www.php.net/distributions/php-%s.tar.xz sha256:%s\n' "$release" "$digest" > "$prefix/SOURCE"
    LD_LIBRARY_PATH="$compat/libxml/lib:$compat/openssl/lib:${LD_LIBRARY_PATH:-}" "$prefix/bin/php" -n -r "exit(PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION === '$version' ? 0 : 1);"
done < "$workspace/packaging/php-sources.txt"
