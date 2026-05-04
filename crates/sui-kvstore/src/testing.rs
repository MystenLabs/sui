// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Reusable test utilities for the BigTable KV Store.
//!
//! Provides a self-contained BigTable emulator lifecycle (spawn, table creation, teardown)
//! for use in integration tests across crates.
//!
//! Requires `gcloud`, `cbt`, and the BigTable emulator on PATH.

use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use futures::future::try_join_all;
use tokio::process::Command as TokioCommand;

pub const INSTANCE_ID: &str = "bigtable_test_instance";

/// New tables must be added here when introduced. The indexer will fail to write to missing
/// tables, so test failures will signal when this list needs updating.
pub const TABLES: &[&str] = &[
    crate::tables::objects::NAME,
    crate::tables::transactions::NAME,
    crate::tables::checkpoints::NAME,
    crate::tables::checkpoints_by_digest::NAME,
    crate::tables::watermarks::NAME,
    crate::tables::epochs::NAME,
    crate::tables::protocol_configs::NAME,
    crate::tables::packages::NAME,
    crate::tables::packages_by_id::NAME,
    crate::tables::packages_by_checkpoint::NAME,
    crate::tables::system_packages::NAME,
    crate::tables::tx_seq_digest::NAME,
];

/// A self-contained BigTable emulator process.
/// Spawns the emulator on a random port.
/// The emulator process is killed when this struct is dropped.
pub struct BigTableEmulator {
    child: Child,
    host: String,
}

impl BigTableEmulator {
    pub fn start() -> Result<Self> {
        let port = get_available_port();
        let child = Command::new(cbtemulator_path())
            .arg(format!("-port={port}"))
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .context("Failed to spawn BigTable emulator")?;

        let host = format!("localhost:{port}");
        Ok(Self { child, host })
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

impl Drop for BigTableEmulator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Bind to an ephemeral port and return it. The port is moved into TIME_WAIT so the OS
/// reserves it briefly, allowing the caller to reuse it with SO_REUSEADDR.
fn get_available_port() -> u16 {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("Failed to bind to ephemeral port");
    let addr = listener.local_addr().expect("Failed to get local address");
    let _sender = std::net::TcpStream::connect(addr).expect("Failed to connect to ephemeral port");
    let _incoming = listener.accept().expect("Failed to accept connection");
    addr.port()
}

/// Resolve the path to `cbtemulator` relative to the gcloud SDK root.
/// Works regardless of whether gcloud was installed via apt, brew, or the standalone installer.
pub fn cbtemulator_path() -> PathBuf {
    let output = Command::new("gcloud")
        .args(["info", "--format=value(installation.sdk_root)"])
        .output()
        .expect("gcloud not found on PATH — install the Google Cloud SDK to run these tests");
    assert!(output.status.success(), "failed to query gcloud sdk root");

    let sdk_root = String::from_utf8(output.stdout)
        .expect("non-utf8 gcloud sdk root")
        .trim()
        .to_string();

    let path = PathBuf::from(sdk_root).join("platform/bigtable-emulator/cbtemulator");
    assert!(
        path.exists(),
        "cbtemulator not found at {path:?} — run: gcloud components install bigtable"
    );
    path
}

pub fn require_bigtable_emulator() {
    cbtemulator_path();
    assert!(
        Command::new("cbt").arg("-version").output().is_ok(),
        "cbt not found on PATH — run: gcloud components install cbt"
    );
}

/// Create all required BigTable tables in parallel using async subprocesses.
pub async fn create_tables(host: &str, instance_id: &str) -> Result<()> {
    try_join_all(TABLES.iter().map(|table| {
        let host = host.to_string();
        let instance_id = instance_id.to_string();
        let table = *table;
        async move {
            let output = TokioCommand::new("cbt")
                .args(["-instance", &instance_id, "-project", "emulator"])
                .arg("createtable")
                .arg(table)
                .env("BIGTABLE_EMULATOR_HOST", &host)
                .output()
                .await
                .with_context(|| format!("Failed to run cbt createtable {table}"))?;
            if !output.status.success() {
                bail!(
                    "cbt createtable {table} failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            let output = TokioCommand::new("cbt")
                .args(["-instance", &instance_id, "-project", "emulator"])
                .args(["createfamily", table, "sui"])
                .env("BIGTABLE_EMULATOR_HOST", &host)
                .output()
                .await
                .with_context(|| format!("Failed to run cbt createfamily {table}"))?;
            if !output.status.success() {
                bail!(
                    "cbt createfamily {table} failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            Ok(())
        }
    }))
    .await?;
    Ok(())
}
