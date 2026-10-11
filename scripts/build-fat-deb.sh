#!/bin/sh
set -eu

workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime_root=${LOCALWEB_RUNTIME_ROOT:-$workspace/packaging/runtime-root}
version=${LOCALWEB_VERSION:-0.1.0}
architecture=$(dpkg --print-architecture)
[ "$architecture" = amd64 ] || {
    echo "The initial LocalWeb package supports amd64 only" >&2
    exit 2
}

for required in \
    runtime-manifest.json \
    nginx/sbin/nginx \
    nginx/conf/mime.types \
    apache/bin/apache2 \
    node/bin/node \
    node/npm/bin/npm-cli.js \
    node/npm/bin/npx-cli.js \
    php/8.5/bin/php \
    php/8.5/sbin/php-fpm \
    composer/2/composer.phar \
    mariadb/bin/mariadbd \
    mariadb/bin/mariadb-install-db \
    mariadb/bin/mariadb \
    mariadb/bin/my_print_defaults \
    mariadb/share/mariadb/fill_help_tables.sql \
    mysql/bin/mysqld \
    mysql/bin/mysql \
    postgres/bin/postgres \
    postgres/bin/initdb \
    postgres/bin/psql \
    postgres/share/postgres.bki \
    mailpit/bin/mailpit \
    redis/bin/redis-server \
    redis/bin/redis-cli; do
    [ -f "$runtime_root/$required" ] || {
        echo "Incomplete fat runtime: missing $runtime_root/$required" >&2
        exit 2
    }
done

cargo build --manifest-path "$workspace/Cargo.toml" --workspace --release --locked

stage=$(mktemp -d /tmp/localweb-deb.XXXXXX)
trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
install -d \
    "$stage/DEBIAN" \
    "$stage/usr/bin" \
    "$stage/usr/lib/localweb" \
    "$stage/usr/lib/systemd/user" \
    "$stage/usr/share/applications" \
    "$stage/usr/share/icons/hicolor/scalable/apps" \
    "$stage/usr/share/polkit-1/actions" \
    "$stage/usr/share/doc/localweb"

for binary in localweb localwebd localweb-gateway localweb-helper localweb-gui; do
    install -m 0755 "$workspace/target/release/$binary" "$stage/usr/lib/localweb/$binary"
    ln -s "../lib/localweb/$binary" "$stage/usr/bin/$binary"
done

cp -a "$runtime_root" "$stage/usr/lib/localweb/runtime"
install -m 0644 "$workspace/packaging/systemd/localwebd.service" "$stage/usr/lib/systemd/user/localwebd.service"
install -m 0644 "$workspace/packaging/polkit/io.localweb.helper.policy" "$stage/usr/share/polkit-1/actions/io.localweb.helper.policy"
install -m 0644 "$workspace/packaging/desktop/io.localweb.LocalWeb.desktop" "$stage/usr/share/applications/io.localweb.LocalWeb.desktop"
install -m 0644 "$workspace/packaging/icons/io.localweb.LocalWeb.svg" "$stage/usr/share/icons/hicolor/scalable/apps/io.localweb.LocalWeb.svg"
install -m 0644 "$workspace/README.md" "$stage/usr/share/doc/localweb/README.md"
install -m 0644 "$workspace/ARCHITECTURE.md" "$stage/usr/share/doc/localweb/ARCHITECTURE.md"
install -m 0644 "$workspace/SECURITY.md" "$stage/usr/share/doc/localweb/SECURITY.md"
install -m 0755 "$workspace/packaging/maintainer/postinst" "$stage/DEBIAN/postinst"
install -m 0755 "$workspace/packaging/maintainer/prerm" "$stage/DEBIAN/prerm"
install -m 0755 "$workspace/packaging/maintainer/postrm" "$stage/DEBIAN/postrm"

installed_size=$(du -sk "$stage/usr" | awk '{print $1}')
cat > "$stage/DEBIAN/control" <<EOF
Package: localweb
Version: $version
Architecture: amd64
Maintainer: Ogalesha <mirgen.pervathi@gmail.com>
Section: devel
Priority: optional
Installed-Size: $installed_size
Depends: libc6 (>= 2.42), libgcc-s1, systemd, polkitd, ca-certificates, libcap2-bin, libx11-6, libxcb1, libxkbcommon0, libwayland-client0, libegl1, libgl1, xdg-utils
Description: Native local web development environment
 LocalWeb bundles nginx, Apache, PHP, Composer, MariaDB, MySQL, PostgreSQL,
 Redis, Node.js with npm/npx, and Mailpit.
 It runs projects as hardened user services without containers or virtual machines.
EOF

source_date_epoch=${SOURCE_DATE_EPOCH:-0}
find "$stage" -print0 | xargs -0 touch -h -d "@$source_date_epoch"
dist_dir=${LOCALWEB_DIST_DIR:-$workspace/dist}
install -d "$dist_dir"
package="$dist_dir/localweb_${version}_amd64.deb"
dpkg-deb --root-owner-group --build "$stage" "$package"
echo "$package"
