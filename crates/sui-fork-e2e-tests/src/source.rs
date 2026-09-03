// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A local Sui network with the GraphQL stack a fork reads from.
//!
//! `sui-fork` resolves everything it does not hold locally through GraphQL pinned at the fork
//! checkpoint, so a source network needs a fullnode, an indexer writing to PostgreSQL, a
//! consistent store for the owned-object and balance enumerations that seeding relies on, and a
//! GraphQL server over both. That is what `sui start --with-graphql` assembles, so
//! [`SourceNetwork`] spawns the `sui` binary under test with that command on ports the harness
//! allocates, and leaves the rest to the scripts: `localnet_setup` in `tests/shell_lib/lib.sh`
//! waits for the faucet to answer, creates the client config, and funds it, all through the same
//! binary.
//!
//! Two habits of `--force-regenesis` are sandboxed. It keeps its config directory under the
//! system temp dir without printing or removing it, so the child runs with `TMPDIR` inside the
//! network's own temp dir, which goes away with the network. And it rewrites the localnet RPC
//! URL of whatever `client.yaml` sits in `SUI_CONFIG_DIR`, so the child gets an empty directory
//! there rather than the developer's `~/.sui/sui_config`.

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use sui_config::local_ip_utils::get_available_port;
use tempfile::TempDir;

use crate::script::release_group;
use crate::script::signal_process;
use crate::script::spawn_in_group;

/// Epoch length of the source network, long enough that no test straddles an epoch change.
const EPOCH_DURATION_MS: u64 = 60 * 60 * 1_000;

/// Longest wait for `sui start` to exit after SIGINT before its process group is killed. A minute,
/// because a group kill cannot reach the PostgreSQL that `sui start` runs in a session of its own,
/// and with a dozen localnets shutting down at once on one machine twenty seconds proved too short.
const STOP_GRACE: Duration = Duration::from_secs(60);

/// Time between the SIGINTs sent to `sui start` while it is still running.
const INTERRUPT_INTERVAL: Duration = Duration::from_secs(2);

/// Interval between checks for the child's exit.
const POLL: Duration = Duration::from_millis(200);

/// Lines of `sui start` output quoted in errors.
const LOG_TAIL_LINES: usize = 40;

/// A running `sui start --force-regenesis --with-graphql --with-faucet`.
///
/// [`Self::stop`] interrupts the child and waits for it, so PostgreSQL and the node databases are
/// shut down before the directory holding them is removed. Dropping a network that is still
/// running kills its process group instead, which covers every early return in the harness.
pub struct SourceNetwork {
    child: Child,
    group: u32,
    rpc_url: String,
    graphql_url: String,
    faucet_url: String,
    log_path: PathBuf,
    _dir: TempDir,
}

impl SourceNetwork {
    /// Spawn `sui start` from the `sui` binary at `sui` and return without waiting for it, since
    /// the scripts poll the faucet until the network answers.
    pub fn start(sui: &Path) -> Result<Self> {
        let dir = tempfile::tempdir().context("failed to create the source network directory")?;
        let tmp_dir = dir.path().join("tmp");
        let sui_config_dir = dir.path().join("sui_config");
        for subdir in [&tmp_dir, &sui_config_dir] {
            std::fs::create_dir(subdir)
                .with_context(|| format!("failed to create {}", subdir.display()))?;
        }
        let log_path = dir.path().join("sui-start.log");
        let log = File::create(&log_path).context("failed to create the sui start log")?;

        let rpc_port = free_loopback_port();
        let graphql_port = free_loopback_port();
        let consistent_store_port = free_loopback_port();
        let faucet_port = free_loopback_port();

        // The `--with-*` flags take `=` and an explicit host, because a bare port binds 0.0.0.0.
        let mut command = Command::new(sui);
        command
            .arg("start")
            .arg("--force-regenesis")
            .args(["--epoch-duration-ms", &EPOCH_DURATION_MS.to_string()])
            .args(["--fullnode-rpc-port", &rpc_port.to_string()])
            .arg(format!("--with-graphql=127.0.0.1:{graphql_port}"))
            .arg(format!(
                "--with-consistent-store=127.0.0.1:{consistent_store_port}"
            ))
            .arg(format!("--with-faucet=127.0.0.1:{faucet_port}"))
            .env("TMPDIR", &tmp_dir)
            .env("SUI_CONFIG_DIR", &sui_config_dir)
            // The binary logs at `error` by default. The `sui` target adds the lines that name
            // each service as it comes up, which is what a failed start needs to be diagnosed.
            .env("RUST_LOG", "error,sui=info")
            .env("RUST_BACKTRACE", "0")
            .stdin(Stdio::null())
            .stdout(
                log.try_clone()
                    .context("failed to clone the sui start log handle")?,
            )
            .stderr(log);
        let child = spawn_in_group(&mut command).context("failed to spawn sui start")?;
        let group = child.id();

        Ok(Self {
            child,
            group,
            rpc_url: format!("http://127.0.0.1:{rpc_port}"),
            graphql_url: format!("http://127.0.0.1:{graphql_port}/graphql"),
            faucet_url: format!("http://127.0.0.1:{faucet_port}/v2/gas"),
            log_path,
            _dir: dir,
        })
    }

    /// Return the fullnode RPC URL, which scripts register as their `localnet` env.
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// Return the GraphQL endpoint, which is what a fork passes as `--network`.
    pub fn graphql_url(&self) -> &str {
        &self.graphql_url
    }

    /// Return the faucet endpoint scripts fund their address from.
    pub fn faucet_url(&self) -> &str {
        &self.faucet_url
    }

    /// Interrupt `sui start` and wait for it to exit, so its services shut down in order and
    /// PostgreSQL is stopped by the process that started it.
    ///
    /// Fails if the child had already exited, because a script then ran against a dead network.
    pub async fn stop(mut self) -> Result<()> {
        if let Some(status) = self.try_wait()? {
            release_group(self.group);
            return Err(anyhow!(
                "sui start exited with {status} during the test:\n{}",
                self.log_tail()
            ));
        }

        // `sui start` listens for SIGINT only between the validator health checks of its main
        // loop, and a signal that lands during a check is lost, so the interrupt is repeated until
        // the child exits. Once it is shutting down, the extra signals are ignored.
        let deadline = Instant::now() + STOP_GRACE;
        let mut next_interrupt = Instant::now();
        let status = loop {
            if let Some(status) = self.try_wait()? {
                break Some(status);
            }
            let now = Instant::now();
            if now >= deadline {
                break None;
            }
            if now >= next_interrupt {
                signal_process(self.child.id(), "INT");
                next_interrupt = now + INTERRUPT_INTERVAL;
            }
            tokio::time::sleep(POLL).await;
        };
        // After a clean exit the group holds at most a PostgreSQL the child failed to stop; after
        // a timeout it still holds the child.
        release_group(self.group);
        match status {
            Some(status) if status.success() => Ok(()),
            Some(status) => Err(anyhow!(
                "sui start exited with {status} on SIGINT:\n{}",
                self.log_tail()
            )),
            None => Err(anyhow!(
                "sui start did not exit within {STOP_GRACE:?} of SIGINT and was killed:\n{}",
                self.log_tail()
            )),
        }
    }

    fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child
            .try_wait()
            .context("failed to wait for sui start")
    }

    /// The last lines of the child's output, for error messages.
    fn log_tail(&self) -> String {
        let log = std::fs::read_to_string(&self.log_path).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
    }
}

impl Drop for SourceNetwork {
    /// Kill the process group of a network that was never stopped, before its directory goes.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            release_group(self.group);
        }
    }
}

/// Reserve a free loopback port the way `sui start` picks its own internal ports.
pub fn free_loopback_port() -> u16 {
    get_available_port("127.0.0.1")
}
