# Runtime provenance (0.1.0)

Historical inputs for the earlier Ubuntu 26.04 reference payload: PHP 8.5
and no PostgreSQL server. This is not the expanded native-build matrix.
Current sources are pinned in `packaging/php-sources.txt` and
`scripts/ci-inputs.sh`; each new package records its actual manifest. The old
Node.js 22 apt entry below is retained for historical reproducibility; current
native packages bundle official Node.js 24.21.0 archives with pinned digests.

## Archive packages (Ubuntu 26.04 `resolute` + `resolute-updates`)

| Component  | Version                | Binary packages                                                                 |
| ---------- | ---------------------- | ------------------------------------------------------------------------------- |
| nginx      | 1.28.3-2ubuntu1.11     | `nginx`                                                                         |
| Apache     | 2.4.66-2ubuntu2.5      | `apache2`, `apache2-bin`                                                        |
| PHP        | 8.5.4-0ubuntu1.3       | system `php8.5` plus `php8.5-fpm`, `php8.5-xml`, `php8.5-mbstring`, `php8.5-intl`, `php8.5-zip`, `php8.5-gd`, `php8.5-bcmath`, `php8.5-curl`, `php8.5-sqlite3`, `php8.5-mysql`, `php8.5-pgsql`, `php8.5-redis`, `php8.5-soap`, `php8.5-igbinary` |
| MariaDB    | 1:11.8.6-5ubuntu0.1    | `mariadb-server`, `mariadb-server-core`, `mariadb-client`, `mariadb-client-core` |
| MySQL      | 8.4.11-0ubuntu0.26.04.1| `mysql-server`, `mysql-server-core`, `mysql-client-core`                        |
| Redis      | 5:8.0.5-1ubuntu0.1     | `redis`, `redis-server`, `redis-tools`                                          |
| Node.js    | 22.22.1+dfsg+~cs22.19.15-1ubuntu1 | `nodejs`, `npm`, plus the modules in `npm-modules.txt`  |
| Support libs | as archived          | `liblzf1`, `libapr1t64`, `libaprutil1t64`, `libaprutil1-dbd-sqlite3`, `libaprutil1-ldap`, `libtcmalloc-minimal4t64` (extracted for library collection only, never installed) |

Nothing is ever installed on the build host: `.deb` files are downloaded
with `apt-get download`, extracted to a scratch directory, and referenced
through `PATH`, `LD_LIBRARY_PATH`, and the `LOCALWEB_*` overrides.

## Upstream artifacts (pinned digests)

| Artifact   | Version | Source                                              | SHA-256                                                            |
| ---------- | ------- | --------------------------------------------------- | ------------------------------------------------------------------ |
| Composer   | 2.10.3  | https://getcomposer.org/download/latest-stable/composer.phar | `7a2d379d5b8ffdaa028580ef26494c36d2feef4b178d3dd1473a4dbc5e17c8d6` |
| Mailpit    | 1.31.5  | https://github.com/axllent/mailpit/releases/download/v1.31.5/mailpit-linux-amd64.tar.gz | tarball `5924ace84f38c83c29f941a644a2d7be6c11f339eb4a9a12ab526414897ceed1`, binary `c9725d03866e243bd243997231103a26e7bc1111b946db0b33b60011ce1f214c` |
| Node.js (Linux x64) | 24.21.0 | https://nodejs.org/dist/v24.21.0/node-v24.21.0-linux-x64.tar.xz | `fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6` |

Composer's digest is published upstream. Mailpit release metadata publishes
archive digests; the binary digest above pins the extracted bytes.

## Staging inputs

The commands below reproduce the archived apt-based payload; the current
native matrix obtains its pinned Node.js 24 archive through `ci-inputs.sh`.

```bash
export COMPOSER_PHAR=... COMPOSER_SHA256=7a2d379d...
export MAILPIT_BIN=...   MAILPIT_SHA256=c9725d03...
export LOCALWEB_NGINX_VERSION=1.28.3-2ubuntu1.11
export LOCALWEB_PHP_VERSION=8.5.4-0ubuntu1.3
export LOCALWEB_MARIADB_VERSION=1:11.8.6-5ubuntu0.1
export LOCALWEB_REDIS_VERSION=5:8.0.5-1ubuntu0.1
export LOCALWEB_APACHE_VERSION=2.4.66-2ubuntu2.5
export LOCALWEB_NODE_VERSION=22.22.1+dfsg+~cs22.19.15-1ubuntu1
export LOCALWEB_MYSQL_VERSION=8.4.11-0ubuntu0.26.04.1
export LOCALWEB_MAILPIT_VERSION=1.31.5
export LOCALWEB_MARIADB_SHARE=<extracted>/usr/share/mariadb
export LOCALWEB_MYSQL_SHARE=<extracted>/usr/share/mysql
export LOCALWEB_PHP_EXTRA_EXTENSIONS=<dir with extra .so files>
export LOCALWEB_APACHE_MODULES=<extracted>/usr/lib/apache2/modules
export LOCALWEB_NPM_SHARE=<extracted>/usr/share/npm
export LOCALWEB_NODEJS_SHARE=<merged>/usr/share/nodejs
SOURCE_DATE_EPOCH=0 ./scripts/stage-ubuntu-runtime.sh
SOURCE_DATE_EPOCH=0 ./scripts/build-fat-deb.sh
```

Staging aborts if any bundled binary has unresolved shared libraries.
