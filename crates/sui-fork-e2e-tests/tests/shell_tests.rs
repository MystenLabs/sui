// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Snapshot tests that drive `sui-fork` and the Sui CLI from shell scripts.
//!
//! Every `.sh` file under `tests/shell_tests` is one test named `shell_tests::<relative path>`.
//! The harness builds the `sui` and `sui-fork` binaries, starts a localnet with GraphQL, copies
//! the script's directory, `tests/shell_lib`, and the Sui framework packages into a sandbox, runs
//! the script with both binaries on `PATH`, and compares the normalised output with the `.snap`
//! file next to the script. The output layout matches `crates/sui/tests/shell_tests.rs` so the
//! two suites read the same way, and snapshots are reviewed the same way, with
//! `cargo insta test -p sui-fork-e2e-tests --test shell_tests --review`.
//!
//! Scripts receive these variables:
//!
//! - `LOCALNET_CONFIG`: the localnet `client.yaml`, whose keystore holds funded addresses and
//!   whose active env is `localnet`.
//! - `FORK_CONFIG`: a copy of that file sharing the same keystore, which scripts extend with a
//!   `fork` env once they know the fork's port.
//! - `LOCALNET_RPC_URL`: the localnet fullnode's RPC URL.
//! - `GRAPHQL_URL`: the localnet GraphQL endpoint, which is the fork's `--network`.
//! - `FORK_DATA_DIR`: an empty directory for `sui-fork start --data-dir`.
//! - `FORK_RPC_ADDR`: a free loopback address for `sui-fork start --rpc-addr`, allocated here
//!   because `start` reports the address it was asked for rather than the port it bound, so an
//!   ephemeral port could not be discovered from a script.
//! - `ACTIVE_ADDRESS`: the funded address scripts seed the fork with.
//!
//! Scripts start the fork themselves through `lib.sh`, redirecting its output to a file so that
//! the fork never holds the script's stdout or stderr pipe.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use fs_extra::dir::CopyOptions;
use move_command_line_common::insta_assert;
use regex::Regex;
use sui_config::SUI_CLIENT_CONFIG;
use sui_fork_e2e_tests::Binaries;
use sui_fork_e2e_tests::ScriptOutput;
use sui_fork_e2e_tests::SourceNetwork;
use sui_fork_e2e_tests::forward_termination_signals;
use sui_fork_e2e_tests::run_script;
use sui_pg_db::temp::get_available_port;

const TEST_DIR: &str = "tests/shell_tests";
const TEST_PATTERN: &str = r"\.sh$";

/// Directory of shared shell helpers, copied into every sandbox root.
const SHELL_LIB_DIR: &str = "tests/shell_lib";

/// Name of the copy of the localnet client config that scripts extend with a `fork` env.
const FORK_CLIENT_CONFIG: &str = "fork.yaml";

/// Deadline for one script, kept below nextest's termination deadline for this package so the
/// harness itself kills a hung script and reports its partial output.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(240);

/// Values the snapshot must not depend on, replaced by stable markers before comparison.
struct Redactions {
    sandbox: PathBuf,
    config_dir: PathBuf,
    fork_data_dir: PathBuf,
    graphql_url: String,
    localnet_rpc_url: String,
    active_address: String,
}

/// Run the bash script at `path` against a fresh localnet and compare its output to the snapshot
/// of the same name.
#[tokio::main]
async fn shell_tests(path: &Path) -> datatest_stable::Result<()> {
    tokio::spawn(forward_termination_signals());
    let binaries = Binaries::build()?;
    let source = SourceNetwork::start().await?;

    let sandbox = tempfile::tempdir()?;
    let fork_data = tempfile::tempdir()?;
    let fork_rpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
    let copy = CopyOptions::new().content_only(true);
    fs_extra::dir::copy(path.parent().unwrap(), sandbox.path(), &copy)?;
    fs_extra::dir::copy(manifest_relative(SHELL_LIB_DIR), sandbox.path(), &copy)?;
    fs_extra::dir::copy(sui_package_dir(), sandbox.path(), &copy)?;

    let localnet_config = source.config_dir().join(SUI_CLIENT_CONFIG);
    let fork_config = source.config_dir().join(FORK_CLIENT_CONFIG);
    std::fs::copy(&localnet_config, &fork_config)
        .context("failed to create the fork client config")?;

    let redactions = Redactions {
        sandbox: sandbox.path().to_path_buf(),
        config_dir: source.config_dir().to_path_buf(),
        fork_data_dir: fork_data.path().to_path_buf(),
        graphql_url: source.graphql_url().to_string(),
        localnet_rpc_url: source.rpc_url().to_owned(),
        active_address: source.active_address().to_string(),
    };

    let mut shell = Command::new("bash");
    shell
        .arg(path.file_name().unwrap())
        .current_dir(sandbox.path())
        .env(
            "PATH",
            binaries.path_with(&std::env::var_os("PATH").unwrap_or_default())?,
        )
        .env("RUST_BACKTRACE", "0")
        .env_remove("RUST_LOG")
        .env("LOCALNET_CONFIG", &localnet_config)
        .env("FORK_CONFIG", &fork_config)
        .env("LOCALNET_RPC_URL", &redactions.localnet_rpc_url)
        .env("GRAPHQL_URL", &redactions.graphql_url)
        .env("FORK_DATA_DIR", fork_data.path())
        .env("FORK_RPC_ADDR", fork_rpc_addr.to_string())
        .env("ACTIVE_ADDRESS", &redactions.active_address);

    let output = tokio::task::spawn_blocking(move || run_script(shell, SCRIPT_TIMEOUT))
        .await
        .context("script runner panicked")??;
    source.stop().await?;

    let result = redact(render(path, &output)?, &redactions);
    if output.timed_out {
        return Err(anyhow!(
            "script exceeded {SCRIPT_TIMEOUT:?} and was killed; output so far:\n{result}"
        )
        .into());
    }

    insta_assert! {
        input_path: path,
        contents: result,
    }
    Ok(())
}

/// Assemble the snapshot body in the same layout as the Sui CLI shell tests.
fn render(path: &Path, output: &ScriptOutput) -> Result<String> {
    Ok(format!(
        "----- script -----\n{}\n----- results -----\nsuccess: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        std::fs::read_to_string(path)?,
        output.status.success(),
        output.status.code().unwrap_or(!0),
        normalise_text(&output.stdout),
        normalise_text(&output.stderr),
    ))
}

fn normalise_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace(r"\\", "/")
        .replace(r"\", "/")
}

/// Replace run-specific values with markers, most specific first.
///
/// The three directories are sibling temp dirs, and the two URLs are replaced before the generic
/// port pattern so that they keep their names in the snapshot. Object ids, digests, checkpoint
/// numbers, and timestamps are left to the scripts, as in the Sui CLI shell tests.
fn redact(text: String, redactions: &Redactions) -> String {
    let text = redact_path(text, &redactions.fork_data_dir, "<FORK_DATA_DIR>");
    let text = redact_path(text, &redactions.config_dir, "<ROOT>");
    let text = redact_path(text, &redactions.sandbox, "<SANDBOX_DIR>");
    let text = text
        .replace(&redactions.graphql_url, "<GRAPHQL_URL>")
        .replace(&redactions.localnet_rpc_url, "<LOCALNET_RPC_URL>")
        .replace(&redactions.active_address, "<ACTIVE_ADDRESS>");
    // Any loopback port that survives belongs to a fork the script bound to an ephemeral port.
    Regex::new(r"(127\.0\.0\.1|localhost):\d+")
        .unwrap()
        .replace_all(&text, "$1:<PORT>")
        .into_owned()
}

/// Replace both the canonical and the raw spelling of `path`, because macOS temp dirs resolve
/// from `/var/folders` to `/private/var/folders` and tools print either form.
fn redact_path(text: String, path: &Path, marker: &str) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    text.replace(canonical.to_string_lossy().as_ref(), marker)
        .replace(path.to_string_lossy().as_ref(), marker)
}

fn manifest_relative(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Return the Sui framework package dir, which scripts publishing Move packages depend on.
fn sui_package_dir() -> PathBuf {
    manifest_relative("../sui-framework/packages")
}

#[cfg(not(msim))]
datatest_stable::harness!(shell_tests, TEST_DIR, TEST_PATTERN);

// The localnet these tests stand up is not exercised by the simulator, so running them under
// msim would only cost time. Expose an empty harness so nextest still sees a well-formed binary.
#[cfg(msim)]
fn main() {
    let _ = shell_tests;
    datatest_stable::runner(&[]);
}
