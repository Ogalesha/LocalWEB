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
import urllib.error
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


def request(kind, data=None, allow_error=False):
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
    if allow_error:
        return response
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
                if request("status")["ready"]:
                    return
            except OSError:
                pass
        assert process.poll() is None, "Daemon did not start"
        time.sleep(0.25)
    raise AssertionError("Daemon did not finish restoring projects")


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
    before_init = mcp_call(mcp, {"jsonrpc": "2.0", "id": 0, "method": "tools/list"})
    assert before_init["error"]["code"] == -32002, before_init
    initialized = mcp_call(mcp, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "acceptance", "version": "1"}}})
    assert initialized["result"]["serverInfo"]["name"] == "localweb"
    mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    mcp.stdin.flush()
    status = mcp_call(mcp, {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "localweb_status", "arguments": {}}})
    assert not status["result"]["isError"], status
    duplicate_init = mcp_call(mcp, {"jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
    assert duplicate_init["error"]["code"] == -32600, duplicate_init
    mcp.terminate()
    mcp.wait(timeout=10)
    mcp = None
    oversized = subprocess.run([str(binary_dir / "localweb-mcp")], input=" " * (1024 * 1024 + 1) + "\n", text=True, capture_output=True, env=env, timeout=10, check=True)
    assert json.loads(oversized.stdout)["error"]["code"] == -32600
    domains = [f"php8-{minor}-{web}-{db}.test" for minor in range(6) for web in ("nginx", "apache") for db in ("mariadb", "mysql", "postgres", "sqlite")]
    domains.append("php-update.test")
    helper("sync-hosts", json.dumps({"address": gateway_address, "domains": domains}).encode())
    if not mac:
        # The helper lock is global, even when callers use different profiles.
        ready = base / "integration-lock.ready"
        release_lock = base / "integration-lock.release"
        locker = subprocess.Popen(["sudo", "python3", "-c",
            "import fcntl,time,sys; from pathlib import Path; "
            "f=open('/etc/.localweb-integration.lock','a'); fcntl.flock(f,fcntl.LOCK_EX); "
            "Path(sys.argv[1]).write_text('ready'); "
            "exec(\"for _ in range(200):\\n if Path(sys.argv[2]).exists(): break\\n time.sleep(.05)\")",
            str(ready), str(release_lock)])
        for _ in range(200):
            if ready.exists():
                break
            time.sleep(0.025)
        assert ready.exists(), "Could not acquire the native integration lock"
        locked = subprocess.Popen(["sudo", "env", f"PKEXEC_UID={uid}", str(binary_dir / "localweb-helper"), "sync-hosts"], stdin=subprocess.PIPE)
        locked.stdin.write(json.dumps({"address": gateway_address, "domains": domains}).encode())
        locked.stdin.close()
        time.sleep(0.2)
        assert locked.poll() is None, "Privileged integration ignored the global lock"
        release_lock.write_text("release")
        locked.wait(timeout=10)
        locker.wait(timeout=10)
        assert locked.returncode == locker.returncode == 0
        print("PASS global privileged integration lock", flush=True)
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

    def fetch(url):
        """Reads a gateway URL, retrying startup failures from a just-restarted service."""
        last = None
        for _ in range(20):
            try:
                with opener.open(url, timeout=20) as response:
                    return response.read().decode()
            except urllib.error.HTTPError as error:
                if error.code not in (502, 503, 504):
                    raise
                last = error
            except (urllib.error.URLError, OSError) as error:
                last = error
            time.sleep(0.25)
        raise AssertionError(f"{url} did not become available: {last}")

    update_name = "php-update"
    update_path = base / update_name
    update_path.mkdir()
    (update_path / "index.php").write_text("<?php echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;", encoding="utf-8")
    update_spec = {"path": str(update_path), "name": update_name, "hostname": f"{update_name}.test", "document_root": ".", "profile": "genericphp", "web_server": "nginx", "php": "8.5", "composer": "2", "node": "24", "database": "none", "cache": "none", "supervisor": {"enabled": False, "command": []}, "cron": [], "cron_preset": False, "mail": False, "autostart": False}
    added = request("project_add", update_spec)
    request("project_start", {"name": update_name})
    web_port = added["project"]["web_port"]
    assert fetch(f"https://{update_name}.test") == "8.5"
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
    assert fetch(f"https://{update_name}.test") == "8.4"
    with opener.open(f"http://127.0.0.1:{web_port}/", timeout=20) as response:
        assert response.read().decode() == "8.4"
    request("project_stop", {"name": update_name})
    stopped = request("project_update", {"name": update_name, "input": changed | {"database": "sqlite", "cache": "none", "mail": False}})
    assert not stopped["running"]
    assert stopped["project"]["database"] == "sqlite"
    assert stopped["project"]["database_port"] is None
    conditional_input = changed | {"database": "sqlite", "cache": "none", "mail": False, "autostart": False}
    conditional = request("project_update_if_current", {"name": update_name, "input": conditional_input, "expected": stopped["project"]})
    assert not conditional["project"]["autostart"]
    rejected = request("project_update_if_current", {"name": update_name, "input": changed, "expected": stopped["project"]}, allow_error=True)
    assert rejected["type"] == "error" and "changed while you were editing" in rejected["data"]["message"], rejected
    assert request("project_get", {"name": update_name})["project"] == conditional["project"]
    redis_config = base / "config/localweb/generated/redis" / f"{update_name}.conf"
    assert redis_config.is_file(), "the Redis configuration was not generated"
    subprocess.check_call([str(binary_dir / "localweb"), "php", update_name, "--", "-r", "exit(0);"], env=env)
    shim_dir = Path(env["XDG_RUNTIME_DIR"]) / "localweb/services/shims" / update_name
    assert shim_dir.is_dir(), "the project command shims were not generated"
    request("project_start", {"name": update_name})
    assert request("project_get", {"name": update_name})["running"]
    request("project_remove", {"name": update_name})
    try:
        request("project_get", {"name": update_name})
    except AssertionError as error:
        assert "not found" in str(error).lower(), error
    else:
        raise AssertionError("removed project is still configured")
    assert not redis_config.exists(), "removal left the generated Redis configuration behind"
    assert not shim_dir.exists(), "removal left the project command shims behind"
    assert (update_path / "index.php").is_file(), "Update or removal touched source files"
    print("PASS php-update: live settings update, restart, database switch, active delete", flush=True)
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
                body = fetch(f"https://{name}.test")
                assert body == f"8.{minor} database OK", f"https://{name}.test returned {body!r}"
                status = request("project_get", {"name": name})
                assert status["running"]
                web_port = status["project"]["web_port"]
                assert web_port, "the project has no direct HTTP port"
                with opener.open(f"http://127.0.0.1:{web_port}/", timeout=20) as response:
                    body = response.read().decode()
                    assert body == f"8.{minor} database OK", f"direct HTTP port {web_port} returned {body!r}"
                if name == "php8-0-nginx-mariadb":
                    if not mac:
                        subprocess.run(["systemctl", "--user", "stop", "localweb-nginx"], env=env, check=True)
                        request("project_start", {"name": name})
                        assert fetch(f"https://{name}.test") == "8.0 database OK", "Start did not recover the stopped web server"
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
                    body = fetch(f"https://{name}.test")
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
    # These fault tests run only on the disposable native CI host.
    fault_projects = []
    for name in ("fault-one", "fault-two"):
        path = base / name
        path.mkdir()
        (path / "index.php").write_text("<?php echo 'owned';")
        spec = {"path": str(path), "name": name, "hostname": f"{name}.test", "php": "8.5", "database": "none", "cache": "redis", "mail": True,
                "supervisor": {"enabled": True, "command": ["@php", "-r", "sleep(300);"]}, "autostart": False}
        request("project_add", spec)
        request("project_start", {"name": name})
        fault_projects.append(request("project_get", {"name": name}))
    config = base / "config/localweb/generated/nginx/nginx.conf"
    backup = config.with_suffix(".saved")
    config.rename(backup)
    config.mkdir()  # atomic config replacement must fail even when CI is root.
    failed = request("project_stop", {"name": "fault-one"}, allow_error=True)
    assert failed["type"] == "error", failed
    for name in ("fault-one", "fault-two"):
        status = request("project_get", {"name": name})
        assert status["recovery_needed"] and not status["running"], status
    config.rmdir()
    backup.rename(config)
    for name in ("fault-one", "fault-two"):
        request("project_stop", {"name": name})
        request("project_start", {"name": name})
    print("PASS failed shared rewrite: all affected projects need recovery", flush=True)

    # A real long-running tool must not block ping, cancellation, or EOF.
    for mode in ("cancel", "eof"):
        mcp = subprocess.Popen([str(binary_dir / "localweb-mcp")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
        mcp_call(mcp, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        marker = base / "fault-one" / f"mcp-{mode}.pid"
        continued = marker.with_suffix(".continued")
        code = f"file_put_contents('{marker}', getmypid()); usleep(1500000); file_put_contents('{continued}', 'leaked'); sleep(30);"
        mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "localweb_project_exec", "arguments": {"name": "fault-one", "command": ["@php", "-r", code]}}}) + "\n")
        mcp.stdin.flush()
        for _ in range(200):
            if marker.exists():
                break
            time.sleep(0.025)
        assert marker.exists(), "MCP command did not start"
        if mode == "cancel":
            ping = mcp_call(mcp, {"jsonrpc": "2.0", "id": 3, "method": "ping"})
            assert ping["id"] == 3 and "result" in ping, ping
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 2}}) + "\n")
            ping = mcp_call(mcp, {"jsonrpc": "2.0", "id": 4, "method": "ping"})
            assert ping["id"] == 4 and "result" in ping, ping
        mcp.stdin.close()
        mcp.wait(timeout=10)
        assert mcp.returncode == 0
        time.sleep(1.7)
        assert not continued.exists(), f"MCP {mode} left its command running"
        mcp = None
    print("PASS MCP control transport: ping, cancellation and EOF", flush=True)

    # Neither invalid nor deleted project files may hide owned processes.
    for index, item in enumerate(fault_projects):
        file = base / "config/localweb/projects" / f"{item['project']['id']}.toml"
        if index == 0:
            file.write_text("invalid = [")
        else:
            file.unlink()
    request("shutdown")
    daemon.wait(timeout=30)
    for item in fault_projects:
        if not mac:
            for kind in ("redis", "mailpit", "supervisor"):
                result = subprocess.run(["systemctl", "--user", "is-active", f"localweb-project-{item['project']['name']}-{kind}"], env=env, capture_output=True)
                assert result.returncode != 0, "Shutdown left an orphan project unit active"
    print("PASS orphan shutdown: malformed and deleted project configs", flush=True)
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
