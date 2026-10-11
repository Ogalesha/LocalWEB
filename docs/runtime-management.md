# Runtime management

LocalWeb uses private native runtimes under `/usr/lib/localweb/runtime`.

## Versions and availability

| Component | Selection / build policy |
| --- | --- |
| PHP | 8.0–8.5, independently selected per project. Full native builds compile all six CLI/FPM pairs from `packaging/php-sources.txt`. PHP 8.6 is excluded until stable. Smaller builds may contain only 8.5. |
| MariaDB | 11.8 LTS in the reference payload; private shared server. |
| MySQL | 8.4 LTS; private shared server. Native Linux CI currently pins the 8.4.7 upstream minimal distribution. |
| PostgreSQL | Native payload from the builder, including `postgres`, `initdb`, `psql`, share files and core extensions. Optional PL/Perl, PL/Python and PL/Tcl modules are omitted because their interpreters are not bundled. Persistent data is separated by PostgreSQL major version. |
| SQLite | PHP `pdo_sqlite` / `sqlite3`; no separate server. |
| Redis | Native builder payload; a cache process per project. |
| nginx / Apache | Native builder payloads; selected per project. |
| Composer | Major 2 only, verified upstream PHAR. |
| Node.js | 24 LTS and its npm/npx payload included; matching system Node.js 22 or 20 can also be selected. Node.js 20 is EOL. |
| Mailpit | Pinned upstream SMTP/inbox binary, isolated per project. |
| Memcached | Not implemented. |

The GUI's PHP selector labels each version as included or requiring CLI/FPM.
An accepted version number does not install the runtime. LocalWeb first
looks in its package, then user runtime storage (`runtimes/php/<version>`),
then matching system binaries. The binaries must match the selected version.
It never silently substitutes PHP 8.5 for another selection.

For external PHP, CLI and FPM plus the app's required extensions must exist.
System paths include versioned Ubuntu binaries and native unversioned Linux
PHP/FPM. A manually supplied private PHP runtime uses `bin/php`, `sbin/php-fpm` and optional `etc/php.ini`.
There is no runtime-download/install command in this version.

## Extensions and commands

Full PHP builds include the common XML, mbstring, curl, intl, ZIP, GD,
bcmath, sockets, SOAP, OpenSSL, OPcache and PDO database capabilities.
The staged payload may also contain additional shared extensions from the
builder. Those modules are not necessarily enabled, and extension enablement
is not exposed as a GUI/CLI setting.

PHP, Composer, workers and `@php` scheduled tasks use the project's selected
executable and its private `php.ini` when present. PHP-FPM uses the same
private configuration. External system PHP keeps its own configuration.

Node.js 20/22 availability is checked when a Node/npm command is requested.
It requires a matching executable and npm entry point in the expected system
location. Starting a PHP site does not itself run Node.

## Provenance and validation

`runtime-manifest.json` records the target OS/architecture, a reproducibility
timestamp from `SOURCE_DATE_EPOCH`, and each component's version, source,
provider and primary-artifact SHA-256. The build verifier checks those hashes,
PHP extensions and npm/npx before packaging. Runtime diagnostics check the
manifest and required paths; they do not repeat hash verification.

CI then tests native service startup and real PDO queries over the complete
PHP/web-server/database matrix. Inspect the successful run for the source
revision you intend to publish. Source provenance for the older Ubuntu 8.5
reference build remains in `packaging/runtime-provenance.md`.
