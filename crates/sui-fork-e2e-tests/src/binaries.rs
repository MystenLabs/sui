// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Locate the `sui` and `sui-fork` binaries by building them with cargo.
//!
//! Cargo exports `CARGO_BIN_EXE_<name>` only to integration tests of the package that owns the
//! binary, and a workspace test run never links the `sui-fork` binary at all because that package
//! has no integration tests. Guessing `target/<profile>/<name>` instead would run whatever was
//! built there last, which silently tests stale code. Building on demand costs one cargo
//! invocation per test process, which is a fingerprint check once the binaries are current, and
//! cargo's build directory lock serialises the test processes that nextest runs in parallel.

use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::ensure;
use serde_json::Value;

/// Paths of the freshly built `sui` and `sui-fork` executables.
#[derive(Clone, Debug)]
pub struct Binaries {
    pub sui: PathBuf,
    pub sui_fork: PathBuf,
}

impl Binaries {
    /// Build both binaries in the profile of the running test and return their paths.
    ///
    /// The result is cached for the life of the process because `cargo test` runs every script in
    /// one process. Errors are cached too, so a broken build is reported once per process rather
    /// than retried for every script.
    pub fn build() -> Result<&'static Self> {
        static BINARIES: OnceLock<Result<Binaries, String>> = OnceLock::new();
        BINARIES
            .get_or_init(|| build_binaries().map_err(|error| format!("{error:#}")))
            .as_ref()
            .map_err(|error| anyhow!("{error}"))
    }

    /// Return `path` with the directories holding `sui` and `sui-fork` prepended.
    pub fn path_with(&self, path: &OsStr) -> Result<OsString> {
        let mut dirs = vec![
            self.sui
                .parent()
                .context("sui has no parent dir")?
                .to_path_buf(),
            self.sui_fork
                .parent()
                .context("sui-fork has no parent dir")?
                .to_path_buf(),
        ];
        dirs.dedup();
        dirs.extend(std::env::split_paths(path));
        std::env::join_paths(dirs).context("PATH entry contains a separator")
    }
}

/// Run `cargo build` for both binaries and read their paths from the JSON artifact messages.
///
/// Reading `executable` from the messages rather than composing `target/<profile>/<name>` keeps
/// the harness correct under a custom `CARGO_TARGET_DIR` or profile, because cargo reports where
/// it actually put the file. Fresh artifacts are reported too, so a no-op build still resolves
/// both paths.
fn build_binaries() -> Result<Binaries> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| env!("CARGO").into());
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(cargo)
        .current_dir(&workspace)
        .args(["build", "--message-format=json-render-diagnostics"])
        .args(["--package", "sui", "--bin", "sui"])
        .args(["--package", "sui-fork", "--bin", "sui-fork"])
        .args(profile_flags()?)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .context("failed to run cargo build for the sui and sui-fork binaries")?;
    ensure!(
        output.status.success(),
        "cargo build of the sui and sui-fork binaries failed with {}",
        output.status
    );

    let mut sui = None;
    let mut sui_fork = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" {
            continue;
        }
        let Some(executable) = message["executable"].as_str() else {
            continue;
        };
        let is_bin = message["target"]["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")));
        if !is_bin {
            continue;
        }
        match message["target"]["name"].as_str() {
            Some("sui") => sui = Some(PathBuf::from(executable)),
            Some("sui-fork") => sui_fork = Some(PathBuf::from(executable)),
            _ => {}
        }
    }

    Ok(Binaries {
        sui: sui.context("cargo build did not report the sui executable")?,
        sui_fork: sui_fork.context("cargo build did not report the sui-fork executable")?,
    })
}

/// Derive cargo profile flags from where the test executable lives.
///
/// Test executables sit at `<target>/<profile dir>/deps/<name>`. The profile directory is `debug`
/// for the dev and test profiles, `release` for release and bench, and the profile's own name
/// otherwise, so building with the matching flag puts the binaries next to the test and at the
/// same optimisation level.
fn profile_flags() -> Result<Vec<String>> {
    let exe = std::env::current_exe().context("cannot locate the test executable")?;
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .context("test executable is not under <target>/<profile>/deps")?;
    Ok(match profile_dir {
        "debug" => vec![],
        "release" => vec!["--release".to_owned()],
        custom => vec!["--profile".to_owned(), custom.to_owned()],
    })
}
