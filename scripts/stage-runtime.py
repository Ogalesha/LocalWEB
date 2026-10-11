#!/usr/bin/env python3
"""Assemble private native runtimes on the target OS; never install packages."""

import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone

WORKSPACE = Path(__file__).resolve().parent.parent
OUT = WORKSPACE / "packaging/runtime-root"
INSTALLED = "/usr/lib/localweb/runtime"
ORIGINS = {}


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT).strip()


def locate(env, *names):
    override = os.environ.get(env)
    if override:
        path = Path(override)
        if path.is_file():
            return path.resolve()
        raise RuntimeError(f"{env} is not a file: {path}")
    for name in names:
        found = shutil.which(name)
        if found:
            return Path(found).resolve()
    raise RuntimeError(f"Set {env}; none of these commands is available: {', '.join(names)}")


def directory(env, *paths):
    choices = [os.environ[env]] if env in os.environ else paths
    for value in choices:
        path = Path(value)
        if path.is_dir():
            return path.resolve()
    raise RuntimeError(f"Set {env} to the runtime directory (tried {choices})")


def copy_file(source, relative):
    dest = OUT / relative
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest, follow_symlinks=True)
    ORIGINS[dest] = Path(source).resolve()
    return dest


def copy_tree(source, relative, ignore=None):
    shutil.copytree(source, OUT / relative, dirs_exist_ok=True, symlinks=False, ignore=ignore)
    for path in (OUT / relative).rglob("*"):
        if path.is_file():
            ORIGINS[path] = (Path(source) / path.relative_to(OUT / relative)).resolve()


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verified(env, checksum_env):
    path = locate(env)
    expected = os.environ.get(checksum_env, "")
    if not re.fullmatch(r"[a-fA-F0-9]{64}", expected) or sha(path).lower() != expected.lower():
        raise RuntimeError(f"{env}: missing or mismatched {checksum_env}")
    return path


def os_target():
    arch = {"x86_64": "amd64"}.get(platform.machine())
    if not arch:
        raise RuntimeError(f"Unsupported architecture: {platform.machine()}")
    values = dict(line.split("=", 1) for line in Path("/etc/os-release").read_text().splitlines() if "=" in line)
    ids = (values.get("ID", "") + " " + values.get("ID_LIKE", "")).replace('"', "").split()
    distro = next((value for value in ("ubuntu", "arch") if value in ids), None)
    if distro is None:
        raise RuntimeError(f"Unsupported build host: {ids}")
    version = values.get("VERSION_ID", "rolling").strip('"') if distro != "arch" else "rolling"
    return f"{distro}-{version}-{arch}"


def dependencies(path):
    result = subprocess.run(["ldd", str(ORIGINS.get(path, path))], capture_output=True, text=True, env=os.environ | {"LD_LIBRARY_PATH": str(OUT / "lib") + ":" + os.environ.get("LD_LIBRARY_PATH", "")})
    if "not found" in result.stdout:
        raise RuntimeError(f"Unresolved libraries in {path}:\n{result.stdout}")
    return re.findall(r"(?:=>\s+)?(/[^\s()]+)", result.stdout)


def bundle_libraries():
    # The upstream MySQL archive includes an older OpenSSL 3 build. Use the
    # target host's ABI-compatible OpenSSL 3 for all native components.
    native_ssl = {}
    for name in ("libssl.so.3", "libcrypto.so.3"):
        source = next((Path(base) / name for base in ("/usr/lib/x86_64-linux-gnu", "/usr/lib64", "/usr/lib") if (Path(base) / name).is_file()), None)
        if source:
            native_ssl[name] = source
            copy_file(source, f"lib/{name}")
    lib_dirs = [Path(value) for value in (
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/lib",
        "/lib64",
        "/lib/x86_64-linux-gnu",
    )]
    if not any((directory / "libncurses.so.6").is_file() for directory in lib_dirs):
        ncursesw = next(
            (directory / "libncursesw.so.6" for directory in lib_dirs
             if (directory / "libncursesw.so.6").is_file()),
            None,
        )
        if ncursesw:
            copy_file(ncursesw, "lib/libncursesw.so.6")
            compat = OUT / "lib/libncurses.so.6"
            compat.symlink_to("libncursesw.so.6")
            ORIGINS[compat] = ncursesw.resolve()
    queue = [path for path in OUT.rglob("*") if path.is_file() and (os.access(path, os.X_OK) or path.suffix in (".so", ".dylib"))]
    checked = set()
    while queue:
        path = queue.pop()
        if path in checked:
            continue
        checked.add(path)
        for dependency in dependencies(path):
            if re.search(r"/(ld-linux[^/]*|lib(c|dl|m|pthread|rt)\.so\..*)$", dependency):
                continue
            source = native_ssl.get(Path(dependency).name, Path(dependency))
            if not source.is_file():
                raise RuntimeError(f"Missing library {dependency} needed by {path}")
            dest = OUT / "lib" / source.name
            if not dest.exists():
                copy_file(source, f"lib/{source.name}")
                queue.append(dest)
            elif sha(source) != sha(dest) and source.resolve() != dest.resolve():
                raise RuntimeError(f"Conflicting private libraries named {source.name}")
    patchelf = shutil.which("patchelf")
    if not patchelf:
        raise RuntimeError("Install patchelf on the native build host")
    for path in checked:
        if path.is_file() and path.read_bytes()[:4] == b"\x7fELF":
            dynamic = subprocess.run(["readelf", "-d", str(path)], capture_output=True, text=True)
            if dynamic.returncode == 0 and re.search(r"\((?:RPATH|RUNPATH)\)", dynamic.stdout):
                subprocess.check_call([patchelf, "--remove-rpath", str(path)])


def main():
    target = os_target()
    composer = verified("COMPOSER_PHAR", "COMPOSER_SHA256")
    mailpit = verified("MAILPIT_BIN", "MAILPIT_SHA256")
    if OUT.exists():
        shutil.rmtree(OUT)
    (OUT / "lib").mkdir(parents=True)
    components = []

    def component(name, path, version, source):
        components.append({"name": name, "version": version, "provider": "bundled-native-build", "source": source, "artifact": str(path.relative_to(OUT))})

    for name, env, candidates, relative, flag in [
        ("nginx", "LOCALWEB_NGINX_BIN", ["nginx"], "nginx/sbin/nginx", "-v"),
        ("apache", "LOCALWEB_APACHE_BIN", ["apache2", "httpd"], "apache/bin/apache2", "-v"),
        ("node", "LOCALWEB_NODE_BIN", ["node"], "node/bin/node", "--version"),
        ("mysql", "LOCALWEB_MYSQL_BIN", ["mysqld"], "mysql/bin/mysqld", "--version"),
        ("mariadb", "LOCALWEB_MARIADB_BIN", ["mariadbd"], "mariadb/bin/mariadbd", "--version"),
        ("redis", "LOCALWEB_REDIS_BIN", ["redis-server"], "redis/bin/redis-server", "--version"),
    ]:
        source = locate(env, *candidates)
        dest = copy_file(source, relative)
        component(name, dest, os.environ.get(f"LOCALWEB_{name.upper()}_VERSION", run(str(source), flag)), f"native:{source}")

    for env, names, dest in [
        ("LOCALWEB_MYSQL_CLIENT", ["mysql"], "mysql/bin/mysql"),
        ("LOCALWEB_MARIADB_CLIENT", ["mariadb"], "mariadb/bin/mariadb"),
        ("LOCALWEB_MARIADB_INIT", ["mariadb-install-db"], "mariadb/bin/mariadb-install-db"),
        ("LOCALWEB_MY_PRINT_DEFAULTS", ["my_print_defaults"], "mariadb/bin/my_print_defaults"),
        ("LOCALWEB_REDIS_CLIENT", ["redis-cli"], "redis/bin/redis-cli"),
    ]:
        copy_file(locate(env, *names), dest)
    if shutil.which("resolveip"):
        copy_file(locate("LOCALWEB_RESOLVEIP", "resolveip"), "mariadb/bin/resolveip")
    (OUT / "mariadb/sbin").mkdir(parents=True)
    (OUT / "mariadb/sbin/mariadbd").symlink_to("../bin/mariadbd")
    (OUT / "mariadb/libexec").mkdir(parents=True)
    (OUT / "mariadb/libexec/mariadbd").symlink_to("../bin/mariadbd")
    copy_tree(directory("LOCALWEB_MARIADB_SHARE", "/usr/share/mariadb", "/usr/share/mysql"), "mariadb/share/mariadb")
    (OUT / "mariadb/share/mysql").symlink_to("mariadb")
    copy_tree(directory("LOCALWEB_MYSQL_SHARE", "/usr/share/mysql"), "mysql/share/mysql")
    for env, default, dest in [("LOCALWEB_MARIADB_PLUGINS", "/usr/lib/mysql/plugin", "mariadb/lib/plugin"), ("LOCALWEB_MYSQL_PLUGINS", "/usr/lib/mysql/plugin", "mysql/lib/plugin")]:
        path = Path(os.environ.get(env, default))
        if path.is_dir():
            # Exclude privileged PAM helpers and optional plugins that need
            # host-only support libraries; LocalWeb does not configure them.
            copy_tree(
                path,
                dest,
                ignore=shutil.ignore_patterns(
                    "auth_pam_tool_dir",
                    "cracklib_password_check.so",
                    "ha_oqgraph.so",
                ),
            )

    pg_config = locate("LOCALWEB_PG_CONFIG", "pg_config")
    pg_bin = directory("LOCALWEB_POSTGRES_BIN", run(str(pg_config), "--bindir"))
    pg_share = directory("LOCALWEB_POSTGRES_SHARE", run(str(pg_config), "--sharedir"))
    pg_lib = directory("LOCALWEB_POSTGRES_LIB", run(str(pg_config), "--pkglibdir"))
    pg_prefix = Path(os.path.commonpath([pg_bin, pg_share, pg_lib]))
    native = Path("postgres/native")
    for name in ["postgres", "initdb", "psql"]:
        copy_file(pg_bin / name, native / pg_bin.relative_to(pg_prefix) / name)
    copy_tree(pg_share, native / pg_share.relative_to(pg_prefix))
    # Procedural-language modules require host interpreters that LocalWeb
    # does not bundle. Keep PostgreSQL's native/core extensions only.
    copy_tree(
        pg_lib,
        native / pg_lib.relative_to(pg_prefix),
        ignore=shutil.ignore_patterns(
            "plperl.so",
            "plperlu.so",
            "*_plperl.so",
            "plpython*.so",
            "*_plpython*.so",
            "pltcl.so",
            "pltclu.so",
            "*_pltcl.so",
        ),
    )
    for label, source in [("bin", pg_bin), ("share", pg_share), ("lib", pg_lib)]:
        (OUT / "postgres" / label).symlink_to(Path("native") / source.relative_to(pg_prefix), target_is_directory=True)
    component("postgres", OUT / "postgres/bin/postgres", run(str(pg_bin / "postgres"), "--version"), f"native:{pg_bin}")

    copy_tree(directory("LOCALWEB_NPM_SHARE", "/usr/share/nodejs/npm", "/usr/share/npm", "/usr/lib/node_modules/npm", "/usr/local/lib/node_modules/npm"), "node/npm")
    shared = os.environ.get("LOCALWEB_NODEJS_SHARE")
    if shared:
        copy_tree(directory("LOCALWEB_NODEJS_SHARE"), "node/npm/node_modules")
    for tool in ("npm", "npx"):
        entry = OUT / f"node/npm/bin/{tool}-cli.js"
        if not entry.is_file():
            raise RuntimeError(f"Incomplete npm payload: {entry}")
        launcher = OUT / f"node/bin/{tool}"
        launcher.write_text(f'#!/bin/sh\nexec "$(dirname "$0")/node" "$(dirname "$0")/../npm/bin/{tool}-cli.js" "$@"\n')
        launcher.chmod(0o755)

    modules = directory("LOCALWEB_APACHE_MODULES", "/usr/lib/apache2/modules", "/usr/lib64/httpd/modules", "/usr/lib/httpd/modules")
    copy_tree(modules, "apache/modules", ignore=shutil.ignore_patterns("mod_lua.so"))
    mime = Path(os.environ["LOCALWEB_MIME_TYPES"]) if "LOCALWEB_MIME_TYPES" in os.environ else next((Path(value) for value in ["/etc/mime.types", "/etc/httpd/conf/mime.types", "/etc/httpd/mime.types"] if Path(value).is_file()), None)
    if mime is None:
        raise RuntimeError("Set LOCALWEB_MIME_TYPES to Apache's MIME types file")
    copy_file(mime, "apache/conf/mime.types")
    mime_lines = ["types {"]
    used_extensions = set()
    for line in mime.read_text().splitlines():
        fields = line.split("#", 1)[0].split()
        if len(fields) < 2:
            continue
        extensions = [value for value in fields[1:] if re.fullmatch(r"[A-Za-z0-9_-]+", value) and value not in used_extensions]
        if extensions:
            used_extensions.update(extensions)
            mime_lines.append(f"    {fields[0]} {' '.join(extensions)};")
    mime_lines.append("}")
    (OUT / "nginx/conf").mkdir(parents=True, exist_ok=True)
    (OUT / "nginx/conf/mime.types").write_text("\n".join(mime_lines) + "\n")

    php_root = os.environ.get("LOCALWEB_PHP_ROOT")
    versions = os.environ.get("LOCALWEB_PHP_VERSIONS", "8.5").split()
    for version in versions:
        if version not in [f"8.{minor}" for minor in range(6)]:
            raise RuntimeError(f"Unsupported PHP version: {version}")
        base = Path(php_root) / version if php_root else None
        php = base / "bin/php" if base else locate("LOCALWEB_PHP_BIN", f"php{version}", "php")
        fpm = base / "sbin/php-fpm" if base else locate("LOCALWEB_FPM_BIN", f"php-fpm{version}", "php-fpm")
        detected = run(str(php), "-n", "-r", 'echo PHP_MAJOR_VERSION.".".PHP_MINOR_VERSION;')
        if detected != version:
            raise RuntimeError(f"Requested PHP {version}, found {detected}")
        dest = copy_file(php, f"php/{version}/bin/php")
        copy_file(fpm, f"php/{version}/sbin/php-fpm")
        extensions = Path(run(str(php), "-n", "-r", 'echo ini_get("extension_dir");'))
        (OUT / f"php/{version}/lib/extensions").mkdir(parents=True)
        if extensions.is_dir():
            copy_tree(extensions, f"php/{version}/lib/extensions")
        extra = os.environ.get("LOCALWEB_PHP_EXTRA_EXTENSIONS") if len(versions) == 1 else None
        if extra:
            copy_tree(Path(extra), f"php/{version}/lib/extensions")
        builtin = {line.lower() for line in run(str(php), "-n", "-m").splitlines()}
        ini = [f"extension_dir={INSTALLED}/php/{version}/lib/extensions", "date.timezone=UTC", "memory_limit=512M", "display_errors=On", "log_errors=On"]
        for name in "pdo mysqlnd mbstring curl intl dom simplexml xml xmlreader xmlwriter zip gd bcmath sockets sqlite3 pdo_sqlite mysqli pdo_mysql pgsql pdo_pgsql soap igbinary redis exif fileinfo ftp gettext ctype tokenizer iconv calendar posix phar".split():
            if name not in builtin and (OUT / f"php/{version}/lib/extensions/{name}.so").is_file():
                ini.append(f"extension={name}.so")
        if "zend opcache" not in builtin and (OUT / f"php/{version}/lib/extensions/opcache.so").is_file():
            ini.append("zend_extension=opcache.so")
        config = OUT / f"php/{version}/etc/php.ini"
        config.parent.mkdir(parents=True)
        config.write_text("\n".join(ini) + "\n")
        source = (base / "SOURCE").read_text().strip() if base and (base / "SOURCE").is_file() else f"native:{php}"
        component(f"php-{version}", dest, run(str(php), "-n", "-r", "echo PHP_VERSION;"), source)
    if "8.5" not in versions:
        raise RuntimeError("The default runtime must include PHP 8.5")
    component("composer", copy_file(composer, "composer/2/composer.phar"), "2", "https://getcomposer.org/download/")
    mailpit_version = os.environ.get("LOCALWEB_MAILPIT_VERSION")
    if not mailpit_version:
        mailpit_version = run(str(mailpit), "--disable-version-check", "version").split()[0]
    component("mailpit", copy_file(mailpit, "mailpit/bin/mailpit"), mailpit_version, "https://github.com/axllent/mailpit")
    bundle_libraries()
    for item in components:
        item["sha256"] = sha(OUT / item.pop("artifact"))
    timestamp = datetime.fromtimestamp(int(os.environ.get("SOURCE_DATE_EPOCH", "0")), timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    (OUT / "runtime-manifest.json").write_text(json.dumps({"schema_version": 1, "target": target, "built_at": timestamp, "components": components}, indent=2) + "\n")
    print(f"Staged {target}: {len(components)} components at {OUT}")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Runtime staging failed: {error}")
