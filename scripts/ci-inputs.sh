#!/bin/sh
set -eu
workspace=$PWD
input="$workspace/packaging/ci-inputs"
mkdir -p "$input"
curl --retry 5 --retry-all-errors --retry-delay 2 -fsSL -o "$input/composer.phar" https://getcomposer.org/download/2.10.3/composer.phar
export COMPOSER_PHAR="$input/composer.phar"
export COMPOSER_SHA256=7a2d379d5b8ffdaa028580ef26494c36d2feef4b178d3dd1473a4dbc5e17c8d6
mail_os=linux; node_os=linux; cpu=amd64; node_cpu=x64
curl --retry 5 --retry-all-errors --retry-delay 2 -fsSL -o "$input/mysql.tar.xz" https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.7-linux-glibc2.28-x86_64-minimal.tar.xz
printf '%s  %s\n' 543ec7d310c4f0786344d243663e5db9e985b03e741ee935370ab08b3274f05a "$input/mysql.tar.xz" | sha256sum -c -
mkdir -p "$input/mysql"
tar -xJf "$input/mysql.tar.xz" --strip-components=1 -C "$input/mysql"
export LOCALWEB_MYSQL_BIN="$input/mysql/bin/mysqld"
export LOCALWEB_MYSQL_CLIENT="$input/mysql/bin/mysql"
export LOCALWEB_MYSQL_SHARE="$input/mysql/share"
export LOCALWEB_MYSQL_PLUGINS="$input/mysql/lib/plugin"
curl --retry 5 --retry-all-errors --retry-delay 2 -fsSL -o "$input/mailpit.tar.gz" "https://github.com/axllent/mailpit/releases/download/v1.31.5/mailpit-$mail_os-$cpu.tar.gz"
python3 - "$input" "$mail_os" "$cpu" <<'PY'
import hashlib, pathlib, sys
root = pathlib.Path(sys.argv[1])
name = f"mailpit-{sys.argv[2]}-{sys.argv[3]}.tar.gz"
checksums = {
    'mailpit-linux-amd64.tar.gz': '5924ace84f38c83c29f941a644a2d7be6c11f339eb4a9a12ab526414897ceed1',
}
assert hashlib.sha256((root / 'mailpit.tar.gz').read_bytes()).hexdigest() == checksums[name]
PY
mkdir -p "$input/mailpit"
tar -xzf "$input/mailpit.tar.gz" -C "$input/mailpit"
export MAILPIT_BIN="$input/mailpit/mailpit"
MAILPIT_SHA256=$(python3 -c 'import hashlib,os; print(hashlib.sha256(open(os.environ["MAILPIT_BIN"],"rb").read()).hexdigest())')
export MAILPIT_SHA256
export LOCALWEB_MAILPIT_VERSION=1.31.5
node_version=24.21.0
node_archive="node-v$node_version-$node_os-$node_cpu.tar.xz"
case "$node_os-$node_cpu" in
    linux-x64) node_sha256=fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6 ;;
    *) echo "unsupported Node.js build target: $node_os-$node_cpu" >&2; exit 2 ;;
esac
curl --retry 5 --retry-all-errors --retry-delay 2 -fsSL -o "$input/$node_archive" "https://nodejs.org/dist/v$node_version/$node_archive"
python3 - "$input/$node_archive" "$node_sha256" <<'PY'
import hashlib, pathlib, sys
archive = pathlib.Path(sys.argv[1])
assert hashlib.sha256(archive.read_bytes()).hexdigest() == sys.argv[2], 'Node.js archive checksum mismatch'
PY
mkdir -p "$input/node"
tar -xJf "$input/$node_archive" --strip-components=1 -C "$input/node"
export LOCALWEB_NODE_BIN="$input/node/bin/node"
export LOCALWEB_NPM_SHARE="$input/node/lib/node_modules/npm"
export LOCALWEB_PHP_ROOT="$workspace/packaging/php-builds"
export LOCALWEB_PHP_VERSIONS="8.0 8.1 8.2 8.3 8.4 8.5"
export LD_LIBRARY_PATH="$workspace/packaging/ci-inputs/php-deps/libxml/lib:$workspace/packaging/ci-inputs/php-deps/openssl/lib:/usr/local/lib:${LD_LIBRARY_PATH:-}"
export DYLD_LIBRARY_PATH="$workspace/packaging/ci-inputs/php-deps/libxml/lib:$workspace/packaging/ci-inputs/php-deps/openssl/lib:${DYLD_LIBRARY_PATH:-}"
