// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use std::fs::OpenOptions;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
use tracing::{event_enabled, info, trace};
use url::Url;

/// A temporary, local postgres database
pub struct TempDb {
    database: LocalDatabase,

    // Directory used for the ephemeral database.
    //
    // On drop the directory will be cleaned an its contents deleted.
    //
    // NOTE: This needs to be the last entry in this struct so that the database is dropped before
    // and has a chance to gracefully shutdown before the directory is deleted.
    dir: tempfile::TempDir,
}

/// Local instance of a `postgres` server.
///
/// See <https://www.postgresql.org/docs/16/app-postgres.html> for more info.
pub struct LocalDatabase {
    dir: PathBuf,
    port: u16,
    url: Url,
    process: Option<PostgresProcess>,
}

#[derive(Debug)]
struct PostgresProcess {
    dir: PathBuf,
    inner: Child,
}

#[derive(Debug)]
enum HealthCheckError {
    NotRunning,
    NotReady,
    #[allow(unused)]
    Unknown(String),
}

impl TempDb {
    /// Create and start a new temporary postgres database.
    ///
    /// A fresh database will be initialized in a temporary directory that will be cleand up on drop.
    /// The running `postgres` service will be serving traffic on an available, os-assigned port.
    pub fn new() -> Result<Self> {
        let dir = tempfile::TempDir::new()?;
        let port = get_available_port();

        let database = LocalDatabase::new_initdb(dir.path().to_owned(), port)?;

        Ok(Self { dir, database })
    }

    pub fn database(&self) -> &LocalDatabase {
        &self.database
    }

    pub fn database_mut(&mut self) -> &mut LocalDatabase {
        &mut self.database
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }
}

impl LocalDatabase {
    /// Start a local `postgres` database service.
    ///
    /// `dir`: The location of the on-disk postgres database. The database must already exist at
    ///     the provided path. If you instead want to create a new database see `Self::new_initdb`.
    ///
    /// `port`: The port to listen for incoming connection on.
    pub fn new(dir: PathBuf, port: u16) -> Result<Self> {
        let url = format!(
            "postgres://postgres:postgrespw@localhost:{port}/{db_name}",
            db_name = "postgres"
        )
        .parse()
        .unwrap();
        let mut db = Self {
            dir,
            port,
            url,
            process: None,
        };
        db.start()?;
        Ok(db)
    }

    /// Initialize and start a local `postgres` database service.
    ///
    /// Unlike `Self::new`, this will initialize a clean database at the provided path.
    pub fn new_initdb(dir: PathBuf, port: u16) -> Result<Self> {
        initdb(&dir)?;
        Self::new(dir, port)
    }

    /// Return the url used to connect to the database
    pub fn url(&self) -> &Url {
        &self.url
    }

    fn start(&mut self) -> Result<()> {
        if self.process.is_none() {
            self.process = Some(PostgresProcess::start(self.dir.clone(), self.port)?);
            self.wait_till_ready()
                .map_err(|e| anyhow!("unable to start postgres: {e:?}"))?;
        }

        Ok(())
    }

    fn health_check(&mut self) -> Result<(), HealthCheckError> {
        if let Some(p) = &mut self.process {
            match p.inner.try_wait() {
                // This would mean the child process has crashed
                Ok(Some(_)) => Err(HealthCheckError::NotRunning),

                // This is the case where the process is still running
                Ok(None) => pg_isready(self.port),

                // Some other unknown error
                Err(e) => Err(HealthCheckError::Unknown(e.to_string())),
            }
        } else {
            Err(HealthCheckError::NotRunning)
        }
    }

    fn wait_till_ready(&mut self) -> Result<(), HealthCheckError> {
        let start = Instant::now();

        while start.elapsed() < Duration::from_secs(10) {
            match self.health_check() {
                Ok(()) => return Ok(()),
                Err(HealthCheckError::NotReady) => {}
                Err(HealthCheckError::NotRunning | HealthCheckError::Unknown(_)) => break,
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        Err(HealthCheckError::Unknown(
            "timeout reached when waiting for service to be ready".to_owned(),
        ))
    }
}

impl PostgresProcess {
    fn start(dir: PathBuf, port: u16) -> Result<Self> {
        let child = Command::new("postgres")
            // Set the data directory to use
            .arg("-D")
            .arg(&dir)
            // Set the port to listen for incoming connections
            .args(["-p", &port.to_string()])
            // Disable creating and listening on a UDS
            .args(["-c", "unix_socket_directories="])
            // pipe stdout and stderr to files located in the data directory
            .stdout(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("stdout"))?,
            )
            .stderr(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("stderr"))?,
            )
            .spawn()
            .context("command not found: postgres")?;

        Ok(Self { dir, inner: child })
    }

    // https://www.postgresql.org/docs/16/app-pg-ctl.html
    fn pg_ctl_stop(&mut self) -> Result<()> {
        let output = Command::new("pg_ctl")
            .arg("stop")
            .arg("-D")
            .arg(&self.dir)
            .arg("-mfast")
            .output()
            .context("command not found: pg_ctl")?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("couldn't shut down postgres"))
        }
    }

    fn dump_stdout_stderr(&self) -> Result<(String, String)> {
        let stdout = std::fs::read_to_string(self.dir.join("stdout"))?;
        let stderr = std::fs::read_to_string(self.dir.join("stderr"))?;

        Ok((stdout, stderr))
    }
}

impl Drop for PostgresProcess {
    // When the Process struct goes out of scope we need to kill the child process
    fn drop(&mut self) {
        info!("dropping postgres");
        // check if the process has already been terminated
        match self.inner.try_wait() {
            // The child process has already terminated, perhaps due to a crash
            Ok(Some(_)) => {}

            // The process is still running so we need to attempt to kill it
            _ => {
                if self.pg_ctl_stop().is_err() {
                    // Couldn't gracefully stop server so we'll just kill it
                    self.inner.kill().expect("postgres couldn't be killed");
                }
                self.inner.wait().unwrap();
            }
        }

        // Dump the contents of stdout/stderr if TRACE is enabled
        if event_enabled!(tracing::Level::TRACE) {
            if let Ok((stdout, stderr)) = self.dump_stdout_stderr() {
                trace!("stdout: {stdout}");
                trace!("stderr: {stderr}");
            }
        }
    }
}

/// Run the postgres `pg_isready` command to get the status of database
///
/// See <https://www.postgresql.org/docs/16/app-pg-isready.html> for more info
fn pg_isready(port: u16) -> Result<(), HealthCheckError> {
    let output = Command::new("pg_isready")
        .arg("--host=localhost")
        .arg("-p")
        .arg(port.to_string())
        .arg("--username=postgres")
        .output()
        .map_err(|e| HealthCheckError::Unknown(format!("command not found: pg_ctl: {e}")))?;

    trace!("pg_isready code: {:?}", output.status.code());
    trace!("pg_isready output: {}", output.stderr.escape_ascii());
    trace!("pg_isready output: {}", output.stdout.escape_ascii());
    if output.status.success() {
        Ok(())
    } else {
        Err(HealthCheckError::NotReady)
    }
}

/// Run the postgres `initdb` command to initialize a database at the provided path
///
/// See <https://www.postgresql.org/docs/16/app-initdb.html> for more info
fn initdb(dir: &Path) -> Result<()> {
    let output = Command::new("initdb")
        .arg("-D")
        .arg(dir)
        .arg("--no-instructions")
        .arg("--username=postgres")
        .output()
        .context("command not found: initdb")?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "unable to initialize database: {:?}",
            String::from_utf8(output.stderr)
        ))
    }
}

/// Return an ephemeral, available port. On unix systems, the port returned will be in the
/// TIME_WAIT state ensuring that the OS won't hand out this port for some grace period.
/// Callers should be able to bind to this port given they use SO_REUSEADDR.
pub fn get_available_port() -> u16 {
    const MAX_PORT_RETRIES: u32 = 1000;

    for _ in 0..MAX_PORT_RETRIES {
        if let Ok(port) = get_ephemeral_port() {
            return port;
        }
    }

    panic!("Error: could not find an available port");
}

fn get_ephemeral_port() -> std::io::Result<u16> {
    // Request a random available port from the OS
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let addr = listener.local_addr()?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = std::net::TcpStream::connect(addr)?;
    let _incoming = listener.accept()?;

    Ok(addr.port())
}
