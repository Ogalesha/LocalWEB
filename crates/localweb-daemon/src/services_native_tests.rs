//! Opt-in checks against the bundled Ubuntu runtime, without systemd or root.

use super::*;
use std::{io::Read as _, os::unix::process::CommandExt as _, process::Child};

struct ManagedChild(Child);

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let group = i32::try_from(self.0.id()).expect("Linux child PID fits i32");
        // SAFETY: spawn creates a dedicated process group with this child's
        // PID. Signal that owned group so web/FPM workers exit with the master.
        unsafe { libc::kill(-group, libc::SIGTERM) };
        thread::sleep(Duration::from_millis(100));
        // SAFETY: the child has not been reaped, so its group ID cannot be
        // reused by an unrelated process before this final cleanup signal.
        unsafe { libc::kill(-group, libc::SIGKILL) };
        let _ = self.0.wait();
    }
}

struct TestDirectory(PathBuf);

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn spawn(executable: &Path, args: &[&str]) -> ManagedChild {
    ManagedChild(
        Command::new(executable)
            .process_group(0)
            .args(args)
            .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path())
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn get(port: u16, path: &str) -> String {
    get_with_host(port, path, "demo.test")
}

fn get_with_host(port: u16, path: &str, host: &str) -> String {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(stream, "GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn check_routing(with_spaces: bool) {
    let base = std::env::temp_dir().join(format!(
        "lw-native-{}",
        &Uuid::new_v4().simple().to_string()[..8]
    ));
    let _directory = TestDirectory(base.clone());
    let paths = Paths {
        config: base.join(if with_spaces {
            "config space"
        } else {
            "config"
        }),
        data: base.join("data"),
        state: base.join(if with_spaces { "state space" } else { "state" }),
        runtime: base.join(if with_spaces { "run space" } else { "run" }),
    };
    paths.create().unwrap();
    let source = base.join(if with_spaces {
        "source space"
    } else {
        ".source"
    });
    fs::create_dir_all(source.join("sub")).unwrap();
    for file in ["index.php", "sub/other.php"] {
        fs::write(
            source.join(file),
            "<?php echo basename($_SERVER['SCRIPT_FILENAME']);",
        )
        .unwrap();
    }
    fs::create_dir_all(source.join(".git")).unwrap();
    fs::create_dir_all(source.join(".well-known")).unwrap();
    fs::write(source.join(".env"), "private-token").unwrap();
    fs::write(source.join(".git/config"), "private-token").unwrap();
    fs::write(source.join(".well-known/check"), "public-check").unwrap();
    fs::write(
        source.join("server.php"),
        "<?php echo json_encode(array_intersect_key($_SERVER, array_flip(['REMOTE_ADDR', 'REMOTE_PORT', 'SERVER_ADDR', 'SERVER_PORT', 'GATEWAY_INTERFACE'])));",
    )
    .unwrap();
    let input = serde_json::from_value(serde_json::json!({
        "path": source, "name": "demo", "hostname": "demo.test",
        "autostart": false, "php": "8.5", "database": "none"
    }))
    .unwrap();
    let mut project = Project::from_new(input, &localweb_core::GlobalConfig::default()).unwrap();
    let services = Services { paths: &paths };
    let runtime = RuntimeLayout::discover();
    let (_, fpm) = services.php_binaries(&project.php).unwrap();
    let fpm_config = services
        .render_php(&project.php, &[project.clone()])
        .unwrap();
    let fpm_process = spawn(&fpm, &["-n", "-F", "-y", fpm_config.to_str().unwrap()]);
    wait_for_file(&services.php_socket(&project), "test PHP-FPM").unwrap();
    for web in [WebServer::Nginx, WebServer::Apache] {
        let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        project.web_port = Some(port);
        project.web_server = web;
        let _web = match web {
            WebServer::Nginx => {
                let config = services.render_nginx(&[project.clone()]).unwrap();
                spawn(
                    &runtime.nginx().unwrap(),
                    &[
                        "-p",
                        runtime.nginx_prefix().to_str().unwrap(),
                        "-c",
                        config.to_str().unwrap(),
                        "-g",
                        "daemon off;",
                    ],
                )
            }
            WebServer::Apache => {
                let config = services.render_apache(&[project.clone()]).unwrap();
                spawn(
                    &runtime.apache().unwrap(),
                    &["-f", config.to_str().unwrap(), "-DFOREGROUND"],
                )
            }
        };
        wait_for_tcp(port, "test web server").unwrap();
        check_responses(web, port);
    }
    check_redis(&services, &project);
    drop(fpm_process);
    fs::remove_dir_all(base).unwrap();
}

fn check_redis(services: &Services<'_>, project: &Project) {
    let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let config = services.render_redis(project, port).unwrap();
    let _redis = spawn(
        &RuntimeLayout::discover().redis().unwrap().0,
        &[config.to_str().unwrap()],
    );
    wait_for_tcp(port, "test Redis").unwrap();
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"PING\r\n").unwrap();
    let mut response = [0; 7];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"+PONG\r\n");
}

fn check_responses(web: WebServer, port: u16) {
    for host in [
        "attacker.invalid",
        "demo.test.attacker.invalid",
        "localhost.attacker.invalid",
    ] {
        let response = get_with_host(port, "/index.php", host);
        assert!(
            response.starts_with("HTTP/1.1 4") || response.starts_with("HTTP/1.0 4"),
            "{web:?} accepted unrelated host {host}: {response}"
        );
    }
    for host in ["127.0.0.1", "localhost"] {
        assert!(
            get_with_host(port, "/index.php", host).ends_with("index.php"),
            "{web:?} rejected direct loopback host {host}"
        );
    }
    let response = get(port, "/server.php");
    let (_, body) = response.split_once("\r\n\r\n").unwrap();
    let server: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(server["REMOTE_ADDR"], "127.0.0.1", "{web:?}");
    assert_eq!(server["SERVER_ADDR"], "127.0.0.1", "{web:?}");
    assert_eq!(server["SERVER_PORT"], port.to_string(), "{web:?}");
    assert_eq!(server["GATEWAY_INTERFACE"], "CGI/1.1", "{web:?}");
    assert!(
        server["REMOTE_PORT"]
            .as_str()
            .unwrap()
            .parse::<u16>()
            .unwrap()
            > 0,
        "{web:?}"
    );
    for (path, script) in [
        ("/index.php", "index.php"),
        ("/sub/other.php", "other.php"),
        ("/a-route?x=1", "index.php"),
    ] {
        let response = get(port, path);
        assert!(
            response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200"),
            "{web:?} {path}: {response}"
        );
        assert!(response.ends_with(script), "{web:?} {path}: {response}");
    }
    for path in ["/.env", "/.git/config", "/missing.php"] {
        let response = get(port, path);
        assert!(
            response.starts_with("HTTP/1.1 40") || response.starts_with("HTTP/1.0 40"),
            "{web:?} {path}: {response}"
        );
        assert!(
            !response.contains("private-token"),
            "{web:?} exposed {path}"
        );
    }
    assert!(get(port, "/.well-known/check").ends_with("public-check"));
}

#[test]
#[ignore = "requires the installed Linux PHP 8.5, nginx and Apache runtime"]
fn bundled_web_servers_execute_php() {
    check_routing(false);
}

#[test]
#[ignore = "requires the installed Linux PHP 8.5, nginx and Apache runtime"]
fn bundled_web_servers_support_paths_with_spaces() {
    check_routing(true);
}

fn initialize_test_database(
    engine: Database,
    services: &Services<'_>,
    data: &Path,
) -> (PathBuf, PathBuf, PathBuf, u16) {
    let layout = RuntimeLayout::discover();
    match engine {
        Database::Mariadb => {
            let (server, initializer, client) = layout.mariadb().unwrap();
            let output = Command::new(initializer)
                .arg("--no-defaults")
                .arg(format!("--basedir={}", layout.mariadb_basedir().display()))
                .arg(format!("--datadir={}", data.display()))
                .args(["--auth-root-authentication-method=normal", "--skip-test-db"])
                .env("LD_LIBRARY_PATH", layout.library_path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            (
                server,
                client,
                services.render_mariadb(data).unwrap(),
                MARIADB_PORT,
            )
        }
        Database::Mysql => {
            let (server, client) = layout.mysql().unwrap();
            let output = Command::new(&server)
                .args(["--no-defaults", "--initialize-insecure"])
                .arg(format!("--datadir={}", data.display()))
                .arg(format!(
                    "--lc-messages-dir={}",
                    layout.mysql_messages_dir().display()
                ))
                .env("LD_LIBRARY_PATH", layout.library_path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            (
                server,
                client,
                services.render_mysql(data).unwrap(),
                MYSQL_PORT,
            )
        }
        _ => panic!("unsupported test engine"),
    }
}

fn check_database_administration(engine: Database) {
    let root = std::env::temp_dir().join(format!(
        "lw-db-auth-{}",
        &Uuid::new_v4().simple().to_string()[..8]
    ));
    let _directory = TestDirectory(root.clone());
    let paths = Paths {
        config: root.join("config"),
        data: root.join("data"),
        state: root.join("state"),
        runtime: root.join("run"),
    };
    paths.create().unwrap();
    let services = Services { paths: &paths };
    let layout = RuntimeLayout::discover();
    let data = root.join("database");
    fs::create_dir(&data).unwrap();
    let (server, client, config, original_port) =
        initialize_test_database(engine, &services, &data);
    let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace(&format!("port={original_port}"), &format!("port={port}")),
    )
    .unwrap();
    for iteration in 0..2 {
        let defaults = format!("--defaults-file={}", config.display());
        let server_process = spawn(&server, &[&defaults]);
        wait_for_tcp(port, "isolated database").unwrap();
        services
            .wait_database_admin(engine)
            .unwrap_or_else(|error| {
                let log_name = if engine == Database::Mariadb {
                    "mariadb.log"
                } else {
                    "mysql.log"
                };
                let log = fs::read_to_string(paths.state.join("logs").join(log_name))
                    .unwrap_or_default()
                    .replace(
                        &services.database_admin_password(engine).unwrap(),
                        "[redacted]",
                    );
                panic!("{error}: {log}");
            });
        let unauthenticated = Command::new(&client)
            .args([
                "--no-defaults",
                "--protocol=tcp",
                "--host=127.0.0.1",
                "--user=root",
                "--skip-password",
            ])
            .arg(format!("--port={port}"))
            .arg("--execute=SELECT 1")
            .env("LD_LIBRARY_PATH", layout.library_path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !unauthenticated.success(),
            "{engine:?} allowed passwordless TCP root"
        );
        let sql = if iteration == 0 {
            "CREATE DATABASE audit; CREATE TABLE audit.marker (value INT); INSERT INTO audit.marker VALUES (42);"
        } else {
            "SELECT value FROM audit.marker;"
        };
        let output = services
            .mysql_admin_command(engine, &client)
            .unwrap()
            .args(["--batch", "--skip-column-names", "--execute", sql])
            .env("LD_LIBRARY_PATH", layout.library_path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if iteration == 1 {
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
        }
        drop(server_process);
    }
}

#[test]
#[ignore = "requires the installed MariaDB runtime; creates an isolated temporary database"]
fn mariadb_rejects_passwordless_root_and_preserves_data_on_restart() {
    check_database_administration(Database::Mariadb);
}

#[test]
#[ignore = "requires the installed MySQL runtime; creates an isolated temporary database"]
fn mysql_rejects_passwordless_root_and_preserves_data_on_restart() {
    check_database_administration(Database::Mysql);
}
