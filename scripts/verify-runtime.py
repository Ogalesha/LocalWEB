#!/usr/bin/env python3
"""Reject incomplete runtime payloads before creating release artifacts."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("root", type=Path)
parser.add_argument("--all-php", action="store_true")
args = parser.parse_args()
root = args.root.resolve()
manifest = json.loads((root / "runtime-manifest.json").read_text())
for component in manifest["components"]:
    assert len(component["sha256"]) == 64, component
    name = component["name"]
    primary = f"php/{name[4:]}/bin/php" if name.startswith("php-") else {"nginx": "nginx/sbin/nginx", "apache": "apache/bin/apache2", "node": "node/bin/node", "mysql": "mysql/bin/mysqld", "mariadb": "mariadb/bin/mariadbd", "postgres": "postgres/bin/postgres", "redis": "redis/bin/redis-server", "composer": "composer/2/composer.phar", "mailpit": "mailpit/bin/mailpit"}[name]
    assert hashlib.sha256((root / primary).read_bytes()).hexdigest() == component["sha256"], f"Manifest hash mismatch: {name}"
versions = [f"8.{minor}" for minor in range(6)] if args.all_php else ["8.5"]
required = ["nginx/sbin/nginx", "apache/bin/apache2", "mariadb/bin/mariadbd", "mariadb/bin/mariadb-install-db", "mariadb/bin/mariadb", "mariadb/libexec/mariadbd", "mysql/bin/mysqld", "mysql/bin/mysql", "postgres/bin/postgres", "postgres/bin/initdb", "postgres/bin/psql", "postgres/share/postgres.bki", "redis/bin/redis-server", "redis/bin/redis-cli", "mailpit/bin/mailpit", "node/bin/node", "node/npm/bin/npm-cli.js", "node/npm/bin/npx-cli.js", "composer/2/composer.phar"]
for version in versions:
    required.extend([f"php/{version}/bin/php", f"php/{version}/sbin/php-fpm", f"php/{version}/etc/php.ini"])
for path in required:
    assert (root / path).is_file(), f"Missing runtime file: {path}"
for version in versions:
    # Override only the temporary extension path to exercise the staged files
    # before the package is installed in its fixed native location.
    ini = (root / f"php/{version}/etc/php.ini").read_text()
    ini = "\n".join(line for line in ini.splitlines() if not line.startswith("extension_dir="))
    with tempfile.NamedTemporaryFile(mode="w", suffix=".ini") as config:
        config.write(ini + f"\nextension_dir={root}/php/{version}/lib/extensions\n")
        config.flush()
        output = subprocess.check_output([str(root / f"php/{version}/bin/php"), "-n", "-c", config.name, "-r", "echo PHP_MAJOR_VERSION,'.',PHP_MINOR_VERSION,PHP_EOL; foreach(['pdo_mysql','pdo_pgsql','pdo_sqlite','mbstring','curl','intl','dom','zip','gd','bcmath'] as $m) { if (!extension_loaded($m)) {fwrite(STDERR, $m.' missing'); exit(1);} }"], text=True)
        assert output.strip() == version, output
subprocess.check_call([str(root / "node/bin/node"), str(root / "node/npm/bin/npm-cli.js"), "--version"])
subprocess.check_call([str(root / "node/bin/node"), str(root / "node/npm/bin/npx-cli.js"), "--version"])
print(f"Runtime verified: {manifest['target']}, PHP {', '.join(versions)}")
