#!/usr/bin/env python3
"""Exercise each PHP/web/database combination against the native service manager.

Run only on a disposable CI host, after installing the newly built package.
The caller supplies administrator authorization for exact test host mappings.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

binary_dir = Path(sys.argv[1]).resolve()
mac = sys.platform == "darwin"
gateway_address = "127.0.0.1" if mac else "127.0.0.2"
base = Path(tempfile.mkdtemp(prefix="lw-accept-", dir=Path.home()))
for name in ("config", "data", "state", "runtime"):
    (base / name).mkdir(mode=0o700)
runtime_parent = base / "runtime" if mac else Path(os.environ["XDG_RUNTIME_DIR"])
env = os.environ | {"XDG_CONFIG_HOME": str(base / "config"), "XDG_DATA_HOME": str(base / "data"), "XDG_STATE_HOME": str(base / "state"), "XDG_RUNTIME_DIR": str(runtime_parent)}
sock = runtime_parent / "localweb/localweb.sock"
uid = subprocess.check_output(["id", "-u"], text=True).strip()
if not mac:
    shim_dir = base / "bin"
    shim_dir.mkdir()
    pkexec = shim_dir / "pkexec"
    pkexec.write_text(
        "#!/bin/sh\n"
        'PKEXEC_UID="${PKEXEC_UID:-$(id -u)}"\n'
        "export PKEXEC_UID\n"
        'exec sudo -n env PKEXEC_UID="$PKEXEC_UID" "$@"\n'
    )
    pkexec.chmod(0o755)
    env["PATH"] = f"{shim_dir}:{env['PATH']}"
    env["PKEXEC_UID"] = uid


def request(kind, data=None):
    payload = {"type": kind}
    if data is not None:
        payload["data"] = data
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(120)
        connection.connect(str(sock))
        connection.sendall(json.dumps({"version": 1, "payload": payload}).encode() + b"\n")
        result = b""
        while not result.endswith(b"\n"):
            chunk = connection.recv(65536)
            assert chunk, "Daemon closed the socket without a response"
            result += chunk
    response = json.loads(result)["payload"]
    assert response["type"] != "error", response
    return response.get("data")


def helper(operation, data):
    subprocess.run(["sudo", "env", f"PKEXEC_UID={uid}", str(binary_dir / "localweb-helper"), operation], input=data, check=True)


def mcp_call(process, payload):
    process.stdin.write(json.dumps(payload) + "\n")
    process.stdin.flush()
    line = process.stdout.readline()
    assert line, "MCP server closed stdout without a reply"
    return json.loads(line)


def wait_for_daemon(process):
    for _ in range(240):
        if sock.is_socket():
            try:
                request("status")
                return
            except OSError:
                pass
        assert process.poll() is None, "Daemon did not start"
        time.sleep(0.25)
    raise AssertionError("Daemon did not accept connections")


mcp = None
daemon = subprocess.Popen([str(binary_dir / "localwebd")], env=env)
try:
    wait_for_daemon(daemon)
    mcp = subprocess.Popen(
        [str(binary_dir / "localweb-mcp")],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
        env=env,
    )
    initialized = mcp_call(mcp, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "acceptance", "version": "1"}}})
    assert initialized["result"]["serverInfo"]["name"] == "localweb"
    status = mcp_call(mcp, {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "localweb_status", "arguments": {}}})
    assert not status["result"]["isError"], status
    mcp.terminate()
    mcp.wait(timeout=10)
    mcp = None
    domains = [f"php8-{minor}-{web}-{db}.test" for minor in range(6) for web in ("nginx", "apache") for db in ("mariadb", "mysql", "postgres", "sqlite")]
    domains.append("php-update.test")
    helper("sync-hosts", json.dumps({"address": gateway_address, "domains": domains}).encode())
    ca = request("ca_status")
    if mac:
        helper("install-ca", Path(ca["certificate"]).read_bytes())
    else:
        subprocess.run(
            [str(binary_dir / "localweb"), "ca", "install"],
            env=env,
            check=True,
        )
    registration = {name: str(base / name / "localweb") for name in ("config", "data", "state")}
    registration["runtime"] = str(runtime_parent / "localweb")
    helper("register-paths", json.dumps(registration).encode())
    ca = request("ca_status")
    assert ca["system_trusted"], ca
    assert ca["browser_trusted"], ca
    assert ca["trusted"], ca
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    update_name = "php-update"
    update_path = base / update_name
    update_path.mkdir()
    (update_path / "index.php").write_text("<?php echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;", encoding="utf-8")
    update_spec = {"path": str(update_path), "name": update_name, "hostname": f"{update_name}.test", "document_root": ".", "profile": "genericphp", "web_server": "nginx", "php": "8.5", "composer": "2", "node": "24", "database": "none", "cache": "none", "supervisor": {"enabled": False, "command": []}, "cron": [], "cron_preset": False, "mail": False, "autostart": False}
    added = request("project_add", update_spec)
    request("project_start", {"name": update_name})
    web_port = added["project"]["web_port"]
    with opener.open(f"https://{update_name}.test", timeout=20) as response:
        assert response.read().decode() == "8.5"
    changed = update_spec | {"web_server": "apache", "php": "8.4", "database": "mariadb", "cache": "redis", "mail": True, "autostart": True}
    updated = request("project_update", {"name": update_name, "input": changed})
    project = updated["project"]
    assert updated["running"], "an active project must restart after an update"
    assert project["id"] == added["project"]["id"]
    assert project["name"] == update_name and project["hostname"] == f"{update_name}.test"
    assert project["path"] == added["project"]["path"]
    assert project["web_port"] == web_port, "the direct web port must survive an update"
    assert project["php"] == "8.4" and project["web_server"] == "apache"
    assert project["database"] == "mariadb" and project["cache"] == "redis"
    assert project["database_port"] and project["database_port"] != project["web_port"]
    with opener.open(f"https://{update_name}.test", timeout=20) as response:
        assert response.read().decode() == "8.4"
    with opener.open(f"http://127.0.0.1:{web_port}/", timeout=20) as response:
        assert response.read().decode() == "8.4"
    request("project_stop", {"name": update_name})
    stopped = request("project_update", {"name": update_name, "input": changed | {"database": "sqlite", "cache": "none", "mail": False}})
    assert not stopped["running"]
    assert stopped["project"]["database"] == "sqlite"
    assert stopped["project"]["database_port"] is None
    request("project_remove", {"name": update_name})
    assert (update_path / "index.php").is_file(), "Update or removal touched source files"
    print("PASS php-update: live settings update, restart, database switch", flush=True)
    for minor in range(6):
        for web in ("nginx", "apache"):
            for db in ("mariadb", "mysql", "postgres", "sqlite"):
                name = f"php8-{minor}-{web}-{db}"
                path = base / name
                path.mkdir()
                spec = {"path": str(path), "name": name, "hostname": f"{name}.test", "document_root": ".", "profile": "genericphp", "web_server": web, "php": f"8.{minor}", "composer": "2", "node": "24", "database": db, "cache": "redis", "supervisor": {"enabled": False, "command": []}, "cron": [], "cron_preset": False, "mail": True, "autostart": False}
                request("project_add", spec)
                values = request("project_environment", {"name": name})
                if db == "sqlite":
                    dsn = "sqlite:" + values["DB_DATABASE"]
                else:
                    driver = "pgsql" if db == "postgres" else "mysql"
                    dsn = f"{driver}:host={values['DB_HOST']};port={values['DB_PORT']};dbname={values['DB_DATABASE']}"
                quote = lambda value: "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"
                source = f"<?php $pdo = new PDO({quote(dsn)}, {quote(values.get('DB_USERNAME', ''))}, {quote(values.get('DB_PASSWORD', ''))}); if ($pdo->query('SELECT 1')->fetchColumn() != 1) {{ http_response_code(500); exit('database failed'); }} echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION.' database OK';"
                (path / "index.php").write_text(source)
                request("project_start", {"name": name})
                with opener.open(f"https://{name}.test", timeout=20) as response:
                    body = response.read().decode()
                    assert body == f"8.{minor} database OK", f"https://{name}.test returned {body!r}"
                status = request("project_get", {"name": name})
                assert status["running"]
                web_port = status["project"]["web_port"]
                assert web_port, "the project has no direct HTTP port"
                with opener.open(f"http://127.0.0.1:{web_port}/", timeout=20) as response:
                    body = response.read().decode()
                    assert body == f"8.{minor} database OK", f"direct HTTP port {web_port} returned {body!r}"
                if name == "php8-0-nginx-mariadb":
                    daemon.kill()
                    daemon.wait(timeout=30)
                    daemon = subprocess.Popen([str(binary_dir / "localwebd")], env=env)
                    wait_for_daemon(daemon)
                    for _ in range(120):
                        if request("project_get", {"name": name})["running"]:
                            break
                        time.sleep(0.5)
                    else:
                        raise AssertionError("the restored project did not become running")
                    with opener.open(f"https://{name}.test", timeout=20) as response:
                        body = response.read().decode()
                        assert body == f"8.{minor} database OK", f"recovered project returned {body!r}"
                    print("PASS daemon SIGKILL recovery: project restored and serving", flush=True)
                try:
                    socket.create_connection(("127.0.0.1", 80), timeout=0.5)
                except OSError:
                    pass
                else:
                    raise AssertionError("LocalWeb must not listen on plain HTTP port 80")
                with socket.create_connection((values["REDIS_HOST"], int(values["REDIS_PORT"])), timeout=5) as redis:
                    redis.sendall(b"PING\r\n")
                    assert redis.recv(32).startswith(b"+PONG")
                with socket.create_connection((values["MAIL_HOST"], int(values["MAIL_PORT"])), timeout=5) as smtp:
                    assert smtp.recv(128).startswith(b"220")
                subprocess.check_call([str(binary_dir / "localweb"), "php", name, "--", "-r", "exit(extension_loaded('pdo_pgsql') ? 0 : 1);"], env=env)
                request("project_stop", {"name": name})
                assert not request("project_get", {"name": name})["running"]
                request("project_remove", {"name": name})
                assert (path / "index.php").is_file(), "Removal touched source files"
                print(f"PASS {name}: HTTPS, PDO, Redis, Mailpit, CLI, stop/remove", flush=True)
    request("shutdown")
    daemon.wait(timeout=30)
    assert daemon.returncode == 0
    print("Acceptance passed: 48 native PHP/web/database combinations")
finally:
    if mcp is not None and mcp.poll() is None:
        mcp.terminate()
        mcp.wait(timeout=10)
    if daemon.poll() is None:
        try:
            request("shutdown")
        finally:
            daemon.wait(timeout=30)
    helper("sync-hosts", json.dumps({"address": gateway_address, "domains": []}).encode())
