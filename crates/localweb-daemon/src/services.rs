use localweb_core::{
    Cache, Database, LocalWebError, Paths, Project, Result, WebServer, atomic_write,
    runtime::RuntimeLayout,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fmt::Write as _,
    fs,
    io::Write as _,
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use uuid::Uuid;

const MARIADB_PORT: u16 = 33_071;
const MYSQL_PORT: u16 = 33_072;
const POSTGRES_PORT: u16 = 33_073;
const MAIL_SMTP_START: u16 = 41_000;
const MAIL_SMTP_END: u16 = 41_999;
const MAIL_HTTP_OFFSET: u16 = 10_000;
const REDIS_PORT_START: u16 = 40_000;
const REDIS_PORT_END: u16 = 49_999;

#[derive(Serialize, Deserialize)]
struct ProjectSecrets {
    schema_version: u32,
    database_name: String,
    database_user: String,
    database_password: String,
    redis_port: Option<u16>,
    #[serde(default)]
    mail_smtp_port: Option<u16>,
}

pub struct Services<'a> {
    pub paths: &'a Paths,
}

impl Services<'_> {
    pub fn php_binaries(&self, version: &str) -> Result<(PathBuf, PathBuf)> {
        localweb_core::validate_php_version(version)?;
        let managed = self.paths.data.join("runtimes/php").join(version);
        let bundled = RuntimeLayout::discover();
        let candidates = vec![
            (
                bundled.root().join("php").join(version).join("bin/php"),
                bundled
                    .root()
                    .join("php")
                    .join(version)
                    .join("sbin/php-fpm"),
            ),
            (managed.join("bin/php"), managed.join("sbin/php-fpm")),
            (
                PathBuf::from(format!("/usr/bin/php{version}")),
                PathBuf::from(format!("/usr/sbin/php-fpm{version}")),
            ),
            (
                PathBuf::from("/usr/bin/php"),
                PathBuf::from("/usr/sbin/php-fpm"),
            ),
            (
                PathBuf::from("/usr/bin/php"),
                PathBuf::from("/usr/bin/php-fpm"),
            ),
        ];
        candidates
            .into_iter()
            .find(|(php, fpm)| {
                php.is_file()
                    && fpm.is_file()
                    && php_matches_version(php, version)
                    && fpm_matches_version(fpm, version)
            })
            .ok_or_else(|| {
                LocalWebError::RuntimeNotInstalled(format!(
                    "PHP {version} CLI and FPM must both be installed"
                ))
            })
    }

    pub fn node_binaries(version: &str) -> Result<(PathBuf, PathBuf)> {
        localweb_core::validate_node_version(version)?;
        let layout = RuntimeLayout::discover();
        if version == "24"
            && let (Ok(node), Ok(npm)) = (layout.node(), layout.npm_cli())
        {
            return Ok((node, npm));
        }
        let system = PathBuf::from("/usr/bin/node");
        if system.is_file() {
            let output = Command::new(&system)
                .arg("-p")
                .arg("process.versions.node.split('.')[0]")
                .output()?;
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == version
                && let Some(npm) = Self::npm_cli_for(&system)
            {
                return Ok((system, npm));
            }
        }
        Err(LocalWebError::RuntimeNotInstalled(format!(
            "Node.js {version} is not available (bundled: 24)"
        )))
    }

    fn npm_cli_for(node: &Path) -> Option<PathBuf> {
        let dir = node.parent()?;
        [
            dir.join("npm-cli.js"),
            dir.join("../npm/bin/npm-cli.js"),
            dir.join("../lib/node_modules/npm/bin/npm-cli.js"),
            PathBuf::from("/usr/share/npm/bin/npm-cli.js"),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())
    }

    pub fn check_project(&self, project: &Project) -> Result<()> {
        project.absolute_document_root()?;
        project.validate_supervisor()?;
        safe_config_path(&project.path)?;
        self.php_binaries(&project.php)?;
        match project.database {
            Database::None => {}
            Database::Mariadb => {
                RuntimeLayout::discover().mariadb()?;
            }
            Database::Mysql => {
                RuntimeLayout::discover().mysql()?;
            }
            Database::Sqlite => {
                self.require_sqlite_support(&project.php)?;
            }
            Database::Postgres => {
                RuntimeLayout::discover().postgres()?;
                self.require_php_module(&project.php, "pdo_pgsql")?;
            }
        }
        match project.cache {
            Cache::None => {}
            Cache::Redis => {
                RuntimeLayout::discover().redis()?;
            }
            Cache::Memcached => {
                return Err(LocalWebError::Service(
                    "Memcached integration is not implemented yet".into(),
                ));
            }
        }
        match project.web_server {
            WebServer::Nginx => {
                RuntimeLayout::discover().nginx()?;
            }
            WebServer::Apache => {
                RuntimeLayout::discover().apache()?;
            }
        }
        if project.mail {
            RuntimeLayout::discover().mailpit().map(|_| ())?;
        }
        Ok(())
    }

    pub fn php_socket(&self, project: &Project) -> PathBuf {
        self.paths
            .runtime
            .join("php")
            .join(&project.php)
            .join(format!("{}.sock", project.name))
    }

    /// Returns the LocalWeb-owned SQLite file for a project.
    ///
    /// The file lives under `LocalWeb`'s data directory so project source
    /// directories are never polluted; an empty file is a valid SQLite
    /// database and most frameworks accept it as-is.
    #[must_use]
    pub fn sqlite_file(&self, project: &Project) -> PathBuf {
        self.paths
            .data
            .join("databases/sqlite")
            .join(format!("{}.sqlite", project.id))
    }

    /// Ensures the SQLite database file exists, creating an empty one.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory or file cannot be created.
    pub fn ensure_sqlite(&self, project: &Project) -> Result<PathBuf> {
        let file = self.sqlite_file(project);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        if !file.is_file() {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)?;
        }
        Ok(file)
    }

    /// Verifies the project's PHP can actually load `pdo_sqlite`.
    ///
    /// # Errors
    ///
    /// Returns an error when PHP cannot run, the extension is absent,
    /// or it fails to load (for example a missing `libsqlite3`).
    fn require_sqlite_support(&self, version: &str) -> Result<()> {
        self.require_php_module(version, "pdo_sqlite")
    }

    fn require_php_module(&self, version: &str, module: &str) -> Result<()> {
        let (php, _) = self.php_binaries(version)?;
        let runtime = RuntimeLayout::discover();
        let mut command = Command::new(&php);
        command
            .arg("-m")
            .env("LD_LIBRARY_PATH", runtime.library_path());
        if let Some(ini) = self.php_ini(version) {
            command.arg("-n").arg("-c").arg(ini);
        }
        let output = command.output()?;
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|loaded| loaded.trim() == module)
        {
            return Ok(());
        }
        Err(LocalWebError::RuntimeNotInstalled(format!(
            "PHP {version} lacks the {module} extension"
        )))
    }

    pub fn php_ini(&self, version: &str) -> Option<PathBuf> {
        [
            RuntimeLayout::discover()
                .root()
                .join("php")
                .join(version)
                .join("etc/php.ini"),
            self.paths
                .data
                .join("runtimes/php")
                .join(version)
                .join("etc/php.ini"),
        ]
        .into_iter()
        .find(|path| path.is_file())
    }

    pub fn php_command_args(&self, version: &str, args: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(ini) = self.php_ini(version) {
            result.extend(["-n".into(), "-c".into(), ini.display().to_string()]);
        }
        result.extend_from_slice(args);
        result
    }

    pub fn render_php(&self, version: &str, projects: &[Project]) -> Result<PathBuf> {
        let dir = self.paths.config.join("generated/php").join(version);
        fs::create_dir_all(&dir)?;
        let runtime = self.paths.runtime.join("php").join(version);
        fs::create_dir_all(&runtime)?;
        let mut pools = String::new();
        for project in projects.iter().filter(|p| p.php == version) {
            safe_config_path(&project.path)?;
            let socket = self.php_socket(project);
            let root = project.absolute_document_root()?;
            write!(pools,
                "[{name}]\nlisten = {socket}\npm = ondemand\npm.max_children = {children}\npm.process_idle_timeout = {idle}s\npm.max_requests = {requests}\nchdir = {root}\ncatch_workers_output = yes\nclear_env = yes\nsecurity.limit_extensions = .php\n\n",
                name = project.name, socket = socket.display(), children = project.resources.fpm_max_children,
                idle = project.resources.fpm_idle_timeout_seconds, requests = project.resources.fpm_max_requests,
                root = root.display(),
            ).map_err(|error| LocalWebError::Config(format!("cannot render PHP-FPM pool: {error}")))?;
        }
        let pool_file = dir.join("pools.conf");
        atomic_write(&pool_file, pools.as_bytes(), 0o600)?;
        let config = format!(
            "[global]\npid = {pid}\nerror_log = {log}\ndaemonize = no\ninclude = {pool}\n",
            pid = runtime.join("php-fpm.pid").display(),
            log = self
                .paths
                .state
                .join(format!("logs/php-{version}.log"))
                .display(),
            pool = pool_file.display(),
        );
        let file = dir.join("php-fpm.conf");
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    pub fn render_nginx(&self, projects: &[Project]) -> Result<PathBuf> {
        let dir = self.paths.config.join("generated/nginx");
        fs::create_dir_all(&dir)?;
        let mut servers = String::new();
        for project in projects.iter().filter(|p| p.web_server == WebServer::Nginx) {
            let root = project.absolute_document_root()?;
            safe_config_path(&root)?;
            let socket = self.php_socket(project);
            write!(servers,
                "server {{\n listen 127.0.0.1:18080;\n server_name {host};\n root \"{root}\";\n index index.php index.html;\n location / {{ try_files $uri $uri/ /index.php?$query_string; }}\n location ~ \\.php$ {{\n  try_files $uri =404;\n  fastcgi_pass unix:{socket};\n  fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n  fastcgi_param SCRIPT_NAME $fastcgi_script_name;\n  fastcgi_param QUERY_STRING $query_string;\n  fastcgi_param REQUEST_METHOD $request_method;\n  fastcgi_param CONTENT_TYPE $content_type;\n  fastcgi_param CONTENT_LENGTH $content_length;\n  fastcgi_param REQUEST_URI $request_uri;\n  fastcgi_param DOCUMENT_URI $document_uri;\n  fastcgi_param DOCUMENT_ROOT $document_root;\n  fastcgi_param SERVER_PROTOCOL $server_protocol;\n  fastcgi_param SERVER_NAME $server_name;\n  fastcgi_param HTTP_HOST $http_host;\n  fastcgi_param HTTPS $fastcgi_https;\n }}\n}}\n",
                host = project.hostname, root = root.display(), socket = socket.display(),
            ).map_err(|error| LocalWebError::Config(format!("cannot render nginx server: {error}")))?;
        }
        let tmp = self.paths.runtime.join("services/nginx-tmp");
        for sub in ["client-body", "proxy", "fastcgi", "uwsgi", "scgi"] {
            fs::create_dir_all(tmp.join(sub))?;
        }
        let config = format!(
            "pid {pid};\nerror_log {error};\nevents {{ worker_connections 256; }}\nhttp {{\n include \"{mime}\";\n access_log {access};\n map $http_x_forwarded_proto $fastcgi_https {{\n default off;\n https on;\n }}\n client_body_temp_path {tmp}/client-body;\n proxy_temp_path {tmp}/proxy;\n fastcgi_temp_path {tmp}/fastcgi;\n uwsgi_temp_path {tmp}/uwsgi;\n scgi_temp_path {tmp}/scgi;\n server {{ listen 127.0.0.1:18080 default_server; return 404; }}\n{servers}\n}}\n",
            mime = RuntimeLayout::discover()
                .root()
                .join("nginx/conf/mime.types")
                .display(),
            pid = self.paths.runtime.join("services/nginx.pid").display(),
            error = self.paths.state.join("logs/nginx-error.log").display(),
            access = self.paths.state.join("logs/nginx-access.log").display(),
            tmp = tmp.display(),
        );
        let file = dir.join("nginx.conf");
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    pub fn render_apache(&self, projects: &[Project]) -> Result<PathBuf> {
        let dir = self.paths.config.join("generated/apache");
        fs::create_dir_all(&dir)?;
        let runtime = RuntimeLayout::discover();
        let server_root = runtime.root().join("apache");
        let run_dir = self.paths.runtime.join("services/apache-run");
        fs::create_dir_all(&run_dir)?;
        let mut vhosts = String::new();
        let mut module_config = String::new();
        for (name, identifier) in [
            ("mpm_event", "mpm_event_module"),
            ("authz_core", "authz_core_module"),
            ("log_config", "log_config_module"),
            ("unixd", "unixd_module"),
            ("mime", "mime_module"),
            ("dir", "dir_module"),
            ("proxy", "proxy_module"),
            ("proxy_fcgi", "proxy_fcgi_module"),
            ("setenvif", "setenvif_module"),
            ("rewrite", "rewrite_module"),
        ] {
            let module = server_root.join("modules").join(format!("mod_{name}.so"));
            if module.is_file() {
                writeln!(
                    module_config,
                    "LoadModule {identifier} \"{}\"",
                    module.display()
                )
                .map_err(|error| LocalWebError::Config(error.to_string()))?;
            }
        }
        for project in projects
            .iter()
            .filter(|p| p.web_server == WebServer::Apache)
        {
            let root = project.absolute_document_root()?;
            safe_config_path(&root)?;
            let socket = self.php_socket(project);
            write!(vhosts,
                "<VirtualHost 127.0.0.1:18081>\n ServerName {host}\n DocumentRoot \"{root}\"\n DirectoryIndex index.php index.html\n SetEnvIf X-Forwarded-Proto \"^https$\" HTTPS=on\n <Directory \"{root}\">\n  AllowOverride None\n  Require all granted\n  RewriteEngine On\n  RewriteCond %{{REQUEST_FILENAME}} !-f\n  RewriteCond %{{REQUEST_FILENAME}} !-d\n  RewriteRule ^ index.php [QSA]\n </Directory>\n ProxyPassMatch ^/(.*\\.php(/.*)?)$ unix:{socket}|fcgi://localhost{root}\n</VirtualHost>\n",
                host = project.hostname, root = root.display(), socket = socket.display(),
            ).map_err(|error| LocalWebError::Config(format!("cannot render Apache vhost: {error}")))?;
        }
        let config = format!(
            "ServerRoot \"{server_root}\"\n{module_config}ServerName localhost\nServerTokens Prod\nTraceEnable off\nListen 127.0.0.1:18081\nDefaultRuntimeDir \"{run}\"\nPidFile \"{pid}\"\nErrorLog \"{error}\"\nLogFormat \"%h %l %u %t '%r' %>s %b\" localweb\nCustomLog \"{access}\" localweb\nTypesConfig \"{mime}\"\n{vhosts}",
            server_root = server_root.display(),
            run = run_dir.display(),
            pid = self.paths.runtime.join("services/apache.pid").display(),
            error = self.paths.state.join("logs/apache-error.log").display(),
            access = self.paths.state.join("logs/apache-access.log").display(),
            mime = server_root.join("conf/mime.types").display(),
        );
        let file = dir.join("httpd.conf");
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    pub fn ensure_mariadb(&self) -> Result<()> {
        let runtime = RuntimeLayout::discover();
        let (server, initializer, _) = runtime.mariadb()?;
        let base = self.paths.data.join("databases/mariadb/11.8");
        let data = base.join("data");
        fs::create_dir_all(&data)?;
        if !data.join("mysql").is_dir() {
            let basedir = runtime.root().join("mariadb");
            let output = Command::new(&initializer)
                .arg("--no-defaults")
                .arg(format!("--basedir={}", basedir.display()))
                .arg(format!("--datadir={}", data.display()))
                .arg("--auth-root-authentication-method=normal")
                .arg("--skip-test-db")
                .env("LD_LIBRARY_PATH", runtime.library_path())
                .output()?;
            if !output.status.success() {
                return Err(LocalWebError::Service(format!(
                    "MariaDB initialization failed: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        let config = self.render_mariadb(&data)?;
        let defaults = format!("--defaults-file={}", config.display());
        Self::start_or_restart("localweb-db-mariadb", &server, &[&defaults])?;
        wait_for_tcp(MARIADB_PORT, "MariaDB")
    }

    pub fn ensure_project_database(&self, project: &Project) -> Result<()> {
        if project.database != Database::Mariadb {
            return Ok(());
        }
        let secrets = self.project_secrets(project)?;
        validate_sql_identifier(&secrets.database_name)?;
        validate_sql_identifier(&secrets.database_user)?;
        if !secrets
            .database_password
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(LocalWebError::Config(
                "generated database password is invalid".into(),
            ));
        }
        let (_, _, client) = RuntimeLayout::discover().mariadb()?;
        let sql = format!(
            "CREATE DATABASE IF NOT EXISTS `{database}`; CREATE USER IF NOT EXISTS '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{database}`.* TO '{user}'@'127.0.0.1'; FLUSH PRIVILEGES;",
            database = secrets.database_name,
            user = secrets.database_user,
            password = secrets.database_password,
        );
        let socket = self.paths.runtime.join("services/mariadb.sock");
        let mut child = Command::new(client)
            .arg("--no-defaults")
            .arg("--protocol=socket")
            .arg(format!("--socket={}", socket.display()))
            .arg("--user=root")
            .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or_else(|| LocalWebError::Service("MariaDB client stdin is unavailable".into()))?
            .write_all(sql.as_bytes())?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "MariaDB project provisioning failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    pub fn ensure_mysql(&self) -> Result<()> {
        let runtime = RuntimeLayout::discover();
        let (server, _) = runtime.mysql()?;
        let base = self.paths.data.join("databases/mysql/8.4");
        let data = base.join("data");
        fs::create_dir_all(&data)?;
        if !data.join("mysql").is_dir() {
            let output = Command::new(&server)
                .arg("--no-defaults")
                .arg("--initialize-insecure")
                .arg(format!(
                    "--lc-messages-dir={}",
                    runtime.root().join("mysql/share/mysql").display()
                ))
                .arg(format!("--datadir={}", data.display()))
                .env("LD_LIBRARY_PATH", runtime.library_path())
                .output()?;
            if !output.status.success() {
                return Err(LocalWebError::Service(format!(
                    "MySQL initialization failed: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        let config = self.render_mysql(&data)?;
        let defaults = format!("--defaults-file={}", config.display());
        Self::start_or_restart("localweb-db-mysql", &server, &[&defaults])?;
        wait_for_tcp(MYSQL_PORT, "MySQL")
    }

    pub fn ensure_project_mysql_database(&self, project: &Project) -> Result<()> {
        if project.database != Database::Mysql {
            return Ok(());
        }
        let secrets = self.project_secrets(project)?;
        validate_sql_identifier(&secrets.database_name)?;
        validate_sql_identifier(&secrets.database_user)?;
        if !secrets
            .database_password
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(LocalWebError::Config(
                "generated database password is invalid".into(),
            ));
        }
        let (_, client) = RuntimeLayout::discover().mysql()?;
        let sql = format!(
            "CREATE DATABASE IF NOT EXISTS `{database}`; CREATE USER IF NOT EXISTS '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{database}`.* TO '{user}'@'127.0.0.1'; FLUSH PRIVILEGES;",
            database = secrets.database_name,
            user = secrets.database_user,
            password = secrets.database_password,
        );
        let socket = self.paths.runtime.join("services/mysql.sock");
        let mut child = Command::new(client)
            .arg("--no-defaults")
            .arg("--protocol=socket")
            .arg(format!("--socket={}", socket.display()))
            .arg("--user=root")
            .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or_else(|| LocalWebError::Service("MySQL client stdin is unavailable".into()))?
            .write_all(sql.as_bytes())?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "MySQL project provisioning failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn render_mysql(&self, data: &Path) -> Result<PathBuf> {
        let directory = self.paths.config.join("generated/mysql");
        fs::create_dir_all(&directory)?;
        let files = self.paths.runtime.join("services/mysql-files");
        fs::create_dir_all(&files)?;
        let runtime = RuntimeLayout::discover();
        let config = format!(
            "[client]\nport={port}\nsocket={socket}\n[mysqld]\ndatadir={data}\nport={port}\nbind-address=127.0.0.1\nsocket={socket}\npid-file={pid}\nlog-error={log}\nlc-messages-dir={messages}\nmax-connections=40\nskip-name-resolve\ndisable-log-bin\nsecure-file-priv={files}\n",
            port = MYSQL_PORT,
            socket = self.paths.runtime.join("services/mysql.sock").display(),
            data = data.display(),
            pid = self.paths.runtime.join("services/mysql.pid").display(),
            log = self.paths.state.join("logs/mysql.log").display(),
            messages = runtime.root().join("mysql/share/mysql").display(),
            files = files.display(),
        );
        let file = directory.join("my.cnf");
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    pub fn ensure_postgres(&self) -> Result<()> {
        let layout = RuntimeLayout::discover();
        let (server, initdb, client) = layout.postgres()?;
        let output = Command::new(&server)
            .arg("--version")
            .env("LD_LIBRARY_PATH", layout.library_path())
            .output()?;
        let version = String::from_utf8_lossy(&output.stdout);
        let major = version
            .split_whitespace()
            .find_map(|value| {
                value
                    .split('.')
                    .next()
                    .filter(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
            })
            .ok_or_else(|| {
                LocalWebError::Config("cannot detect PostgreSQL major version".into())
            })?;
        let data = self.paths.data.join("databases/postgres").join(major);
        fs::create_dir_all(&data)?;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700))?;
        if !data.join("PG_VERSION").is_file() {
            let output = Command::new(initdb)
                .arg("-D")
                .arg(&data)
                .arg("-L")
                .arg(layout.root().join("postgres/share"))
                .args([
                    "--username=localweb_admin",
                    "--encoding=UTF8",
                    "--locale=C",
                    "--auth-local=trust",
                    "--auth-host=scram-sha-256",
                ])
                .env("LD_LIBRARY_PATH", layout.library_path())
                .output()?;
            if !output.status.success() {
                return Err(LocalWebError::Service(format!(
                    "PostgreSQL initialization failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        let socket = self.paths.runtime.join("services/postgres");
        fs::create_dir_all(&socket)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o700))?;
        let data = data.display().to_string();
        let socket = format!("unix_socket_directories={}", socket.display());
        let libraries = format!(
            "dynamic_library_path={}",
            layout.root().join("postgres/lib").display()
        );
        Self::start_or_restart(
            "localweb-db-postgres",
            &server,
            &[
                "-D",
                &data,
                "-c",
                "listen_addresses=127.0.0.1",
                "-c",
                "port=33073",
                "-c",
                &socket,
                "-c",
                "unix_socket_permissions=0700",
                "-c",
                &libraries,
            ],
        )?;
        let library_path = layout.library_path();
        wait_for_postgres(
            &client,
            &self.paths.runtime.join("services/postgres"),
            &library_path,
        )
    }

    pub fn ensure_project_postgres_database(&self, project: &Project) -> Result<()> {
        let secrets = self.project_secrets(project)?;
        let sql = postgres_provision_sql(&secrets)?;
        let layout = RuntimeLayout::discover();
        let (_, _, psql) = layout.postgres()?;
        let mut child = Command::new(psql)
            .args([
                "--no-psqlrc",
                "--set=ON_ERROR_STOP=1",
                "--username=localweb_admin",
                "--dbname=postgres",
                "--port=33073",
                "--host",
            ])
            .arg(self.paths.runtime.join("services/postgres"))
            .env("LD_LIBRARY_PATH", layout.library_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or_else(|| LocalWebError::Service("PostgreSQL client stdin unavailable".into()))?
            .write_all(sql.as_bytes())?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "PostgreSQL project provisioning failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    pub fn start_redis(&self, project: &Project) -> Result<u16> {
        let (server, _) = RuntimeLayout::discover().redis()?;
        let secrets = self.project_secrets(project)?;
        let port = secrets
            .redis_port
            .ok_or_else(|| LocalWebError::Config("Redis project port was not allocated".into()))?;
        let config = self.render_redis(project, port)?;
        let config = config
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 Redis configuration path".into()))?;
        Self::start_or_restart(&redis_unit(project), &server, &[config])?;
        wait_for_tcp(port, "Redis")?;
        Ok(port)
    }

    pub fn stop_redis(project: &Project) -> Result<()> {
        Self::stop(&redis_unit(project))
    }

    pub fn start_mailpit(&self, project: &Project) -> Result<(u16, u16)> {
        let server = RuntimeLayout::discover().mailpit()?;
        let secrets = self.project_secrets(project)?;
        let smtp = secrets.mail_smtp_port.ok_or_else(|| {
            LocalWebError::Config("Mailpit project port was not allocated".into())
        })?;
        let http = smtp + MAIL_HTTP_OFFSET;
        let working = self
            .paths
            .runtime
            .join("services")
            .join(format!("mailpit-{}", project.name));
        fs::create_dir_all(&working)?;
        let data = working.join("mailpit.db");
        let data_str = data
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 Mailpit data path".into()))?;
        let smtp_addr = format!("127.0.0.1:{smtp}");
        let http_addr = format!("127.0.0.1:{http}");
        Self::start_or_restart(
            &mailpit_unit(project),
            &server,
            &[
                "--smtp",
                smtp_addr.as_str(),
                "--listen",
                http_addr.as_str(),
                "--database",
                data_str,
            ],
        )?;
        wait_for_tcp(smtp, "Mailpit SMTP")?;
        Ok((smtp, http))
    }

    pub fn stop_mailpit(project: &Project) -> Result<()> {
        Self::stop(&mailpit_unit(project))
    }

    fn allocate_mail_port(&self) -> Result<u16> {
        let used = self.used_mail_ports()?;
        for port in MAIL_SMTP_START..=MAIL_SMTP_END {
            if !used.contains(&port)
                && TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
                && TcpListener::bind(SocketAddrV4::new(
                    Ipv4Addr::LOCALHOST,
                    port + MAIL_HTTP_OFFSET,
                ))
                .is_ok()
            {
                return Ok(port);
            }
        }
        Err(LocalWebError::Service(
            "no free LocalWeb mail port is available".into(),
        ))
    }

    fn used_mail_ports(&self) -> Result<BTreeSet<u16>> {
        let directory = self.paths.data.join("databases/projects");
        if !directory.is_dir() {
            return Ok(BTreeSet::new());
        }
        let mut ports = BTreeSet::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                let secrets: ProjectSecrets = toml::from_str(&fs::read_to_string(entry.path())?)
                    .map_err(|error| LocalWebError::Config(error.to_string()))?;
                if let Some(smtp) = secrets.mail_smtp_port {
                    ports.insert(smtp);
                    ports.insert(smtp + MAIL_HTTP_OFFSET);
                }
            }
        }
        Ok(ports)
    }

    pub fn start_supervisor(&self, project: &Project) -> Result<()> {
        project.validate_supervisor()?;
        if !project.supervisor.enabled {
            return Ok(());
        }
        let executable = if project.supervisor.command[0] == "@php" {
            self.php_binaries(&project.php)?.0
        } else {
            let requested = PathBuf::from(&project.supervisor.command[0]);
            let candidate = if requested.is_absolute() {
                requested
            } else {
                project.path.join(requested)
            };
            let executable = candidate.canonicalize().map_err(|error| {
                LocalWebError::Config(format!(
                    "cannot resolve supervisor executable {}: {error}",
                    candidate.display()
                ))
            })?;
            if !Path::new(&project.supervisor.command[0]).is_absolute()
                && !executable.starts_with(&project.path)
            {
                return Err(LocalWebError::Config(
                    "relative supervisor executable escapes the project directory".into(),
                ));
            }
            executable
        };
        if !executable.is_file() {
            return Err(LocalWebError::RuntimeNotInstalled(
                executable.display().to_string(),
            ));
        }

        let unit = supervisor_unit(project);
        Self::stop(&unit)?;
        let worker_args = if project.supervisor.command[0] == "@php" {
            self.php_command_args(&project.php, &project.supervisor.command[1..])
        } else {
            project.supervisor.command[1..].to_vec()
        };
        let arguments = supervisor_systemd_args(
            &unit,
            &project.name,
            &project.path,
            &executable,
            &worker_args,
            &RuntimeLayout::discover().library_path(),
        );
        let output = Command::new("systemd-run").args(arguments).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "systemd-run {unit}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        wait_for_unit_active(&unit, "project supervisor")
    }

    pub fn stop_supervisor(project: &Project) -> Result<()> {
        Self::stop(&supervisor_unit(project))
    }

    pub fn environment(&self, project: &Project) -> Result<BTreeMap<String, String>> {
        let mut values = BTreeMap::new();
        if project.database == Database::Mariadb
            || project.database == Database::Mysql
            || project.database == Database::Postgres
            || project.cache == Cache::Redis
            || project.mail
        {
            let secrets = self.project_secrets(project)?;
            if matches!(
                project.database,
                Database::Mariadb | Database::Mysql | Database::Postgres
            ) {
                let port = match project.database {
                    Database::Mysql => MYSQL_PORT,
                    Database::Postgres => POSTGRES_PORT,
                    _ => MARIADB_PORT,
                };
                values.insert(
                    "DB_CONNECTION".into(),
                    if project.database == Database::Postgres {
                        "pgsql"
                    } else {
                        "mysql"
                    }
                    .into(),
                );
                values.insert("DB_HOST".into(), "127.0.0.1".into());
                values.insert("DB_PORT".into(), port.to_string());
                values.insert("DB_DATABASE".into(), secrets.database_name);
                values.insert("DB_USERNAME".into(), secrets.database_user);
                values.insert("DB_PASSWORD".into(), secrets.database_password);
            }
            if project.cache == Cache::Redis {
                let port = secrets.redis_port.ok_or_else(|| {
                    LocalWebError::Config("Redis project port was not allocated".into())
                })?;
                values.insert("REDIS_HOST".into(), "127.0.0.1".into());
                values.insert("REDIS_PORT".into(), port.to_string());
            }
            if project.mail {
                let smtp = secrets.mail_smtp_port.ok_or_else(|| {
                    LocalWebError::Config("Mailpit project port was not allocated".into())
                })?;
                values.insert("MAIL_MAILER".into(), "smtp".into());
                values.insert("MAIL_HOST".into(), "127.0.0.1".into());
                values.insert("MAIL_PORT".into(), smtp.to_string());
                values.insert(
                    "MAIL_WEB_URL".into(),
                    format!("http://127.0.0.1:{}", smtp + MAIL_HTTP_OFFSET),
                );
            }
        }
        if project.database == Database::Sqlite {
            values.insert("DB_CONNECTION".into(), "sqlite".into());
            values.insert(
                "DB_DATABASE".into(),
                self.sqlite_file(project).display().to_string(),
            );
        }
        Ok(values)
    }

    fn project_secrets(&self, project: &Project) -> Result<ProjectSecrets> {
        let directory = self.paths.data.join("databases/projects");
        fs::create_dir_all(&directory)?;
        let file = directory.join(format!("{}.toml", project.id));
        if file.is_file() {
            return toml::from_str(&fs::read_to_string(file)?)
                .map_err(|error| LocalWebError::Config(error.to_string()));
        }
        let database_name = project.name.replace('-', "_");
        validate_sql_identifier(&database_name)?;
        let database_user = format!(
            "{}_{}",
            database_name.chars().take(16).collect::<String>(),
            &project.id.simple().to_string()[..8]
        );
        let database_password = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let redis_port = (project.cache == Cache::Redis)
            .then(|| self.allocate_redis_port())
            .transpose()?;
        let mail_smtp_port = project
            .mail
            .then(|| self.allocate_mail_port())
            .transpose()?;
        let secrets = ProjectSecrets {
            schema_version: 1,
            database_name,
            database_user,
            database_password,
            redis_port,
            mail_smtp_port,
        };
        let encoded = toml::to_string_pretty(&secrets)
            .map_err(|error| LocalWebError::Config(error.to_string()))?;
        atomic_write(&file, encoded.as_bytes(), 0o600)?;
        Ok(secrets)
    }

    fn allocate_redis_port(&self) -> Result<u16> {
        let mut used = self.used_redis_ports()?;
        used.extend(self.used_mail_ports()?);
        for port in REDIS_PORT_START..=REDIS_PORT_END {
            if !used.contains(&port)
                && TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
            {
                return Ok(port);
            }
        }
        Err(LocalWebError::Service(
            "no free LocalWeb Redis port is available".into(),
        ))
    }

    fn used_redis_ports(&self) -> Result<BTreeSet<u16>> {
        let directory = self.paths.data.join("databases/projects");
        if !directory.is_dir() {
            return Ok(BTreeSet::new());
        }
        let mut ports = BTreeSet::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                let secrets: ProjectSecrets = toml::from_str(&fs::read_to_string(entry.path())?)
                    .map_err(|error| LocalWebError::Config(error.to_string()))?;
                ports.extend(secrets.redis_port);
            }
        }
        Ok(ports)
    }

    fn render_mariadb(&self, data: &Path) -> Result<PathBuf> {
        let directory = self.paths.config.join("generated/mariadb");
        fs::create_dir_all(&directory)?;
        let runtime = RuntimeLayout::discover();
        let config = format!(
            "[client]\nport={port}\nsocket={socket}\n[mysqld]\ndatadir={data}\nport={port}\nbind-address=127.0.0.1\nsocket={socket}\npid-file={pid}\nlog-error={log}\nplugin-dir={plugins}\nlc-messages-dir={messages}\nmax-connections=40\nskip-name-resolve\n",
            port = MARIADB_PORT,
            socket = self.paths.runtime.join("services/mariadb.sock").display(),
            data = data.display(),
            pid = self.paths.runtime.join("services/mariadb.pid").display(),
            log = self.paths.state.join("logs/mariadb.log").display(),
            plugins = runtime.root().join("mariadb/lib/plugin").display(),
            messages = runtime.root().join("mariadb/share/mariadb").display(),
        );
        let file = directory.join("my.cnf");
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    fn render_redis(&self, project: &Project, port: u16) -> Result<PathBuf> {
        let directory = self.paths.config.join("generated/redis");
        fs::create_dir_all(&directory)?;
        let working = self
            .paths
            .runtime
            .join("services")
            .join(format!("redis-{}", project.name));
        fs::create_dir_all(&working)?;
        let config = format!(
            "bind 127.0.0.1\nprotected-mode yes\nport {port}\ndaemonize no\nsupervised no\nsave \"\"\nappendonly no\nmaxmemory {memory}mb\nmaxmemory-policy allkeys-lru\ndir {working}\npidfile {pid}\nlogfile {log}\n",
            memory = project.resources.cache_memory_mb,
            working = working.display(),
            pid = working.join("redis.pid").display(),
            log = self
                .paths
                .state
                .join(format!("logs/{}-redis.log", project.name))
                .display(),
        );
        let file = directory.join(format!("{}.conf", project.name));
        atomic_write(&file, config.as_bytes(), 0o600)?;
        Ok(file)
    }

    pub fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
        if !executable.is_file() {
            return Err(LocalWebError::RuntimeNotInstalled(
                executable.display().to_string(),
            ));
        }
        if is_active(unit) {
            run_command("systemctl", &["--user", "restart", unit])?;
        } else {
            let name = format!("--unit={unit}");
            let executable = executable
                .to_str()
                .ok_or_else(|| LocalWebError::Config("non-UTF-8 executable path".into()))?;
            let mut command = Command::new("systemd-run");
            let library_path = format!(
                "--setenv=LD_LIBRARY_PATH={}",
                RuntimeLayout::discover().library_path()
            );
            command.args([
                "--user",
                &name,
                "--collect",
                "--property=StartLimitIntervalSec=0",
            ]);
            // The gateway keeps its file capability: both NoNewPrivileges
            // and PrivateTmp (via a user namespace) would neutralise it.
            if unit != "localweb-gateway" {
                command.arg("--property=NoNewPrivileges=yes");
                command.arg("--property=PrivateTmp=yes");
            }
            command.arg(&library_path);
            for key in [
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "XDG_RUNTIME_DIR",
                "LOCALWEB_RUNTIME_ROOT",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    let mut argument = OsString::from(format!("--setenv={key}="));
                    argument.push(value);
                    command.arg(argument);
                }
            }
            command.args(["--", executable]);
            command.args(args);
            let output = command.output()?;
            if !output.status.success() {
                return Err(LocalWebError::Service(format!(
                    "systemd-run {unit}: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        Ok(())
    }

    pub fn stop(unit: &str) -> Result<()> {
        if is_active(unit) {
            run_command("systemctl", &["--user", "stop", unit])?;
        }
        Ok(())
    }
}

pub fn php_unit(version: &str) -> String {
    format!("localweb-php-{}", version.replace('.', "-"))
}

pub fn redis_unit(project: &Project) -> String {
    format!("localweb-project-{}-redis", project.name)
}

pub fn mailpit_unit(project: &Project) -> String {
    format!("localweb-project-{}-mailpit", project.name)
}

pub fn supervisor_unit(project: &Project) -> String {
    format!("localweb-project-{}-supervisor", project.name)
}

fn supervisor_systemd_args(
    unit: &str,
    project_name: &str,
    working_directory: &Path,
    executable: &Path,
    worker_args: &[String],
    library_path: &str,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("--user"),
        OsString::from(format!("--unit={unit}")),
        OsString::from("--collect"),
        OsString::from(format!("--description=LocalWeb worker for {project_name}")),
        OsString::from(format!(
            "--working-directory={}",
            working_directory.display()
        )),
        OsString::from("--property=NoNewPrivileges=yes"),
        OsString::from("--property=PrivateTmp=yes"),
        OsString::from("--property=Restart=always"),
        OsString::from("--property=RestartSec=2s"),
        OsString::from("--property=KillMode=control-group"),
        OsString::from(format!("--setenv=LD_LIBRARY_PATH={library_path}")),
        OsString::from("--"),
        executable.as_os_str().to_owned(),
    ];
    arguments.extend(worker_args.iter().map(OsString::from));
    arguments
}

fn validate_sql_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty()
        || identifier.len() > 64
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !identifier
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
    {
        return Err(LocalWebError::Config(format!(
            "invalid managed SQL identifier: {identifier}"
        )));
    }
    Ok(())
}

fn php_matches_version(php: &Path, version: &str) -> bool {
    Command::new(php)
        .args(["-n", "-r", "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;"])
        .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path())
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == version
        })
}

fn fpm_matches_version(fpm: &Path, version: &str) -> bool {
    Command::new(fpm)
        .args(["-n", "-v"])
        .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path())
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&format!("PHP {version}."))
        })
}

fn postgres_provision_sql(secrets: &ProjectSecrets) -> Result<String> {
    validate_sql_identifier(&secrets.database_name)?;
    validate_sql_identifier(&secrets.database_user)?;
    if secrets.database_password.is_empty()
        || !secrets
            .database_password
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(LocalWebError::Config(
            "generated PostgreSQL password is invalid".into(),
        ));
    }
    Ok(format!(
        "DO $localweb$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{user}') THEN CREATE ROLE \"{user}\" LOGIN; END IF; END $localweb$;\nALTER ROLE \"{user}\" LOGIN PASSWORD '{password}';\nSELECT format('CREATE DATABASE %I OWNER %I', '{database}', '{user}') WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '{database}')\n\\gexec\nREVOKE CONNECT ON DATABASE \"{database}\" FROM PUBLIC;\nGRANT CONNECT ON DATABASE \"{database}\" TO \"{user}\";\n",
        user = secrets.database_user,
        password = secrets.database_password,
        database = secrets.database_name
    ))
}

fn wait_for_tcp(port: u16, service: &str) -> Result<()> {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    for _ in 0..50 {
        if TcpStream::connect_timeout(&address.into(), Duration::from_millis(100)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(LocalWebError::Service(format!(
        "{service} did not become ready on 127.0.0.1:{port}"
    )))
}

fn wait_for_postgres(client: &Path, socket_dir: &Path, library_path: &str) -> Result<()> {
    for _ in 0..100 {
        let ready = Command::new(client)
            .args([
                "--no-psqlrc",
                "--no-password",
                "--set=ON_ERROR_STOP=1",
                "--username=localweb_admin",
                "--dbname=postgres",
                "--host",
            ])
            .arg(socket_dir)
            .arg(format!("--port={POSTGRES_PORT}"))
            .args([
                "--quiet",
                "--tuples-only",
                "--no-align",
                "--command=SELECT 1",
            ])
            .env("LD_LIBRARY_PATH", library_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if ready {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(LocalWebError::Service(
        "PostgreSQL did not become ready on its project socket".into(),
    ))
}

pub fn wait_for_unit_active(unit: &str, service: &str) -> Result<()> {
    for _ in 0..20 {
        if is_active(unit) {
            thread::sleep(Duration::from_millis(200));
            if is_active(unit) {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(LocalWebError::Service(format!(
        "{service} did not remain active in {unit}; inspect it with `journalctl --user -u {unit}`"
    )))
}

pub fn is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(LocalWebError::Service(format!(
            "{program} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn safe_config_path(path: &Path) -> Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| LocalWebError::Config("non-UTF-8 project path".into()))?;
    if text
        .bytes()
        .any(|b| b.is_ascii_control() || matches!(b, b'"' | b'\\' | b'$' | b';' | b'{' | b'}'))
    {
        return Err(LocalWebError::Config(
            "project path contains characters unsafe for native service configuration".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_config_injection() {
        assert!(safe_config_path(Path::new("/tmp/evil\nlisten=0.0.0.0")).is_err());
        assert!(safe_config_path(Path::new("/tmp/safe site")).is_ok());
    }

    #[test]
    fn sql_identifiers_are_conservative() {
        assert!(validate_sql_identifier("acme").is_ok());
        assert!(validate_sql_identifier("shop_2026").is_ok());
        assert!(validate_sql_identifier("9shop").is_err());
        assert!(validate_sql_identifier("shop`; DROP DATABASE mysql;--").is_err());
    }

    #[test]
    fn postgres_provisioning_rejects_password_injection() {
        let mut secrets = ProjectSecrets {
            schema_version: 1,
            database_name: "acme".into(),
            database_user: "acme".into(),
            database_password: "generated123".into(),
            redis_port: None,
            mail_smtp_port: None,
        };
        let sql = postgres_provision_sql(&secrets).unwrap();
        assert!(sql.contains("REVOKE CONNECT ON DATABASE \"acme\" FROM PUBLIC"));
        assert!(sql.contains("\\gexec"));
        secrets.database_password = "'; DROP DATABASE postgres; --".into();
        assert!(postgres_provision_sql(&secrets).is_err());
    }

    #[test]
    fn supervisor_arguments_never_pass_through_a_shell() {
        let arguments = supervisor_systemd_args(
            "localweb-project-demo-supervisor",
            "demo",
            Path::new("/tmp/demo project"),
            Path::new("/usr/bin/php8.5"),
            &["artisan".into(), "queue:work; touch /tmp/unsafe".into()],
            "/usr/lib/localweb/runtime/lib",
        );
        assert!(!arguments.iter().any(|argument| argument == "sh"));
        assert!(!arguments.iter().any(|argument| argument == "-c"));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "queue:work; touch /tmp/unsafe")
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--working-directory=/tmp/demo project")
        );
    }
}
