#!/bin/sh
set -eu

workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fixture=$(mktemp -d /tmp/localweb-package-fixture.XXXXXX)
output=$(mktemp -d /tmp/localweb-package-output.XXXXXX)
trap 'rm -rf -- "$fixture" "$output"' EXIT HUP INT TERM

for relative in \
    nginx/sbin/nginx \
    nginx/conf/mime.types \
    apache/bin/apache2 \
    node/bin/node \
    node/npm/bin/npm-cli.js \
    node/npm/bin/npx-cli.js \
    php/8.5/bin/php \
    php/8.5/sbin/php-fpm \
    mariadb/bin/mariadbd \
    mariadb/bin/mariadb-install-db \
    mariadb/bin/mariadb \
    mariadb/bin/my_print_defaults \
    mariadb/bin/resolveip \
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
    install -D -m 0755 /bin/true "$fixture/$relative"
done
install -D -m 0644 "$workspace/packaging/runtime-manifest.example.json" "$fixture/runtime-manifest.json"
install -D -m 0644 "$workspace/packaging/runtime-manifest.example.json" "$fixture/composer/2/composer.phar"

LOCALWEB_RUNTIME_ROOT="$fixture" LOCALWEB_DIST_DIR="$output" SOURCE_DATE_EPOCH=0 \
    "$workspace/scripts/build-fat-deb.sh" >/dev/null

package="$output/localweb_0.1.0_amd64.deb"
dpkg-deb --info "$package" >/dev/null
for installed in \
    ./usr/lib/localweb/localwebd \
    ./usr/lib/localweb/localweb-gui \
    ./usr/lib/localweb/localweb-mcp \
    ./usr/lib/localweb/runtime/php/8.5/bin/php \
    ./usr/lib/systemd/user/localwebd.service \
    ./usr/share/applications/io.localweb.LocalWeb.desktop \
    ./usr/share/polkit-1/actions/io.localweb.helper.policy; do
    dpkg-deb --fsys-tarfile "$package" | tar -tf - | grep -Fx "$installed" >/dev/null
done
for maintainer in ./prerm ./postrm; do
    dpkg-deb --ctrl-tarfile "$package" | tar -tf - | grep -Fx "$maintainer" >/dev/null
done

if dpkg-deb --fsys-tarfile "$package" | tar -tf - | grep -Ei '(^|/)(PRODUCT\.md|DESIGN\.md|TASKS\.md|AGENTS\.md|Dockerfile|docker-compose[^/]*|\.github)(/|$)'; then
    echo "Unexpected internal or CI file in the installed package" >&2
    exit 1
fi

echo "Package layout test passed"
