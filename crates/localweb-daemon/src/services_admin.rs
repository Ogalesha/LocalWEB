//! Private database administration credentials, separate from app credentials.

use super::*;

impl Services<'_> {
    pub(super) fn database_admin_password(&self, engine: Database) -> Result<String> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let name = match engine {
            Database::Mariadb => "mariadb",
            Database::Mysql => "mysql",
            Database::Postgres => "postgres",
            _ => {
                return Err(LocalWebError::Config(
                    "database has no administrator".into(),
                ));
            }
        };
        let directory = self.paths.data.join("databases/admin");
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let file = directory.join(format!("{name}.secret"));
        if file.exists() {
            let password = fs::read_to_string(&file)?;
            if password.len() != 64 || !password.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                return Err(LocalWebError::Config(
                    "invalid database administrator credential file".into(),
                ));
            }
            return Ok(password);
        }
        let password = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        atomic_write(&file, password.as_bytes(), 0o600)?;
        Ok(password)
    }

    pub(super) fn mysql_admin_files(&self, engine: Database) -> Result<(PathBuf, PathBuf)> {
        let (name, port) = match engine {
            Database::Mariadb => ("mariadb", MARIADB_PORT),
            Database::Mysql => ("mysql", MYSQL_PORT),
            _ => {
                return Err(LocalWebError::Config(
                    "not a MySQL-compatible database".into(),
                ));
            }
        };
        let password = self.database_admin_password(engine)?;
        let directory = self.paths.config.join("generated").join(name);
        fs::create_dir_all(&directory)?;
        let mut sql = String::new();
        for host in ["localhost", "127.0.0.1"] {
            writeln!(
                sql,
                "CREATE USER IF NOT EXISTS 'root'@'{host}' IDENTIFIED BY '{password}';"
            )
            .unwrap();
            writeln!(
                sql,
                "ALTER USER 'root'@'{host}' IDENTIFIED BY '{password}';"
            )
            .unwrap();
            writeln!(
                sql,
                "GRANT ALL PRIVILEGES ON *.* TO 'root'@'{host}' WITH GRANT OPTION;"
            )
            .unwrap();
        }
        sql.push_str("DROP USER IF EXISTS 'root'@'%', 'root'@'::1', ''@'localhost', ''@'127.0.0.1', ''@'%';\nFLUSH PRIVILEGES;\n");
        let init = directory.join("admin-init.sql");
        atomic_write(&init, sql.as_bytes(), 0o600)?;
        #[cfg(unix)]
        let endpoint = format!(
            "protocol=socket\nsocket=\"{}\"\n",
            config_path(&self.paths.runtime.join(format!("services/{name}.sock")))
        );
        #[cfg(windows)]
        let endpoint = "protocol=tcp\nhost=127.0.0.1\n".to_owned();
        let client = directory.join("admin-client.cnf");
        atomic_write(
            &client,
            format!("[client]\nuser=root\npassword={password}\nport={port}\n{endpoint}").as_bytes(),
            0o600,
        )?;
        Ok((init, client))
    }

    pub(super) fn mysql_admin_command(&self, engine: Database, client: &Path) -> Result<Command> {
        let (_, config) = self.mysql_admin_files(engine)?;
        let mut command = Command::new(client);
        command.arg(format!("--defaults-file={}", config.display()));
        if engine == Database::Mysql {
            command.arg("--no-login-paths");
        }
        Ok(command)
    }

    pub(super) fn database_admin_ready(&self, engine: Database) -> Result<bool> {
        let layout = RuntimeLayout::discover();
        let client = if engine == Database::Mariadb {
            layout.mariadb()?.2
        } else {
            layout.mysql()?.1
        };
        Ok(self
            .mysql_admin_command(engine, &client)?
            .args(["--batch", "--skip-column-names", "--execute=SELECT 1"])
            .env("LD_LIBRARY_PATH", layout.library_path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success()))
    }

    pub(super) fn wait_database_admin(&self, engine: Database) -> Result<()> {
        for _ in 0..100 {
            if self.database_admin_ready(engine)? {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(LocalWebError::Service(
            "database administrator authentication did not become ready".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_private_stable_and_separate_for_each_engine() {
        let (base, paths, mut project) = super::super::tests::fixture();
        project.database_port = Some(19000);
        let services = Services { paths: &paths };
        let password = services.database_admin_password(Database::Mariadb).unwrap();
        assert_eq!(
            password,
            services.database_admin_password(Database::Mariadb).unwrap()
        );
        assert_ne!(
            password,
            services.database_admin_password(Database::Mysql).unwrap()
        );
        let (init, client) = services.mysql_admin_files(Database::Mariadb).unwrap();
        assert!(
            fs::read_to_string(init)
                .unwrap()
                .contains("ALTER USER 'root'@'127.0.0.1'")
        );
        let client_text = fs::read_to_string(&client).unwrap();
        assert!(client_text.contains(&password));
        assert!(
            !services
                .environment(&project)
                .unwrap()
                .values()
                .any(|value| value == &password)
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(client).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(base).unwrap();
    }
}
