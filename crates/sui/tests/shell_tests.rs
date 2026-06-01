// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fs_extra::dir::CopyOptions;
use insta_cmd::get_cargo_bin;
use move_command_line_common::insta_assert;
use std::path::{Path, PathBuf};
use std::process::Command;
use sui_config::{Config, SUI_CLIENT_CONFIG, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{FileBasedKeystore, Keystore};
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use tempfile::TempDir;

// [shell_test_snapshot] is run on every file matching [TEST_PATTERN] in [TEST_DIR].
// Files in [TEST_NET_DIR] will be run with a [TestCluster] configured.
//
// These run the files as shell scripts and compares their output to the snapshots; use `cargo
// insta test --review` to update the snapshots.

const TEST_DIR: &str = "tests/shell_tests";
const TEST_NET_DIR: &str = "tests/shell_tests/with_network";
const TEST_PATTERN: &str = r"\.sh$";

/// run the bash script at [path], comparing its output to the insta snapshot of the same name.
/// The script is run in a temporary working directory that contains a copy of the parent directory
/// of [path], with the `sui` binary on the path.
///
/// The `CONFIG` environment variable is set to a client config file appropriate for the test:
/// - For `with_network` tests: a copy of the shared cluster config from
///   `SUI_TEST_CLUSTER_CONFIG_DIR` (set by the nextest setup script).
/// - For other tests: a temporary config with a bogus RPC URL (see [make_temp_config_dir]).
fn shell_tests(path: &Path) -> datatest_stable::Result<()> {
    let is_network_test = path.starts_with(TEST_NET_DIR);

    // copy files into temporary directory
    let srcdir = path.parent().unwrap();
    let tmpdir = tempfile::tempdir()?;
    let sandbox = tmpdir.path();

    let sui_package_dir_src = get_sui_package_dir();

    // TODO DVX-1950 If you have gitignored files it can affect the snapshots, so we should only
    // copy non-ignored files
    fs_extra::dir::copy(srcdir, sandbox, &CopyOptions::new().content_only(true))?;
    fs_extra::dir::copy(
        sui_package_dir_src,
        sandbox,
        &CopyOptions::new().content_only(true),
    )?;

    // set up command
    let mut shell = Command::new("bash");
    shell
        .env(
            "PATH",
            format!("{}:{}", get_sui_bin_path(), std::env::var("PATH")?),
        )
        .env("RUST_BACKTRACE", "0")
        .current_dir(sandbox)
        .arg(path.file_name().unwrap());

    let shared_config_dir = std::env::var("SUI_TEST_CLUSTER_CONFIG_DIR").ok();
    let temp_config_dir =
        if let Some(ref shared_dir) = shared_config_dir.filter(|_| is_network_test) {
            copy_shared_cluster_config(Path::new(shared_dir))
        } else {
            make_temp_config_dir()
        };
    shell.env("CONFIG", temp_config_dir.path().join(SUI_CLIENT_CONFIG));

    // run it; snapshot test output
    let output = shell.output()?;
    let result = format!(
        "----- script -----\n{}\n----- results -----\nsuccess: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        std::fs::read_to_string(path)?,
        output.status.success(),
        output.status.code().unwrap_or(!0),
        // Convert windows path outputs on the snapshot to regular linux ones.
        String::from_utf8_lossy(&output.stdout)
            .replace(r"\\", "/")
            .replace(r"\", "/"),
        String::from_utf8_lossy(&output.stderr)
            .replace(r"\\", "/")
            .replace(r"\", "/"),
    );

    let result = result
        // redact the temporary directory path
        .replace(temp_config_dir.path().to_string_lossy().as_ref(), "<ROOT>")
        // Redact the sandbox directory path so we can retain snapshots easily.
        // We canonicalize also to make sure we catch absolute paths too.
        .replace(
            sandbox.canonicalize().unwrap().to_string_lossy().as_ref(),
            "<SANDBOX_DIR>",
        )
        .replace(sandbox.to_string_lossy().as_ref(), "<SANDBOX_DIR>");

    insta_assert! {
        input_path: path,
        contents: result,
    }
    Ok(())
}

/// Create a config directory containing a single environment called "testnet" with no cached
/// chain ID and a bogus RPC URL
fn make_temp_config_dir() -> TempDir {
    let result = tempfile::tempdir().expect("can create temp file");
    let config_dir = result.path();

    SuiClientConfig {
        keystore: Keystore::from(
            FileBasedKeystore::load_or_create(&config_dir.join(SUI_KEYSTORE_FILENAME)).unwrap(),
        ),
        external_keys: None,
        envs: vec![SuiEnv {
            alias: "testnet".to_string(),
            rpc: "bogus rpc".to_string(),
            ws: None,
            basic_auth: None,
            chain_id: None,
        }],
        active_env: Some("testnet".to_string()),
        active_address: None,
    }
    .persisted(&result.path().join(SUI_CLIENT_CONFIG))
    .save()
    .expect("can write to tempfile");
    result
}

/// Copy the client config and keystore from a shared cluster config directory into a fresh
/// temporary directory, so each test has its own mutable copy.
fn copy_shared_cluster_config(shared_dir: &Path) -> TempDir {
    let result = tempfile::tempdir().expect("can create temp dir");
    let dst = result.path();
    std::fs::copy(
        shared_dir.join(SUI_CLIENT_CONFIG),
        dst.join(SUI_CLIENT_CONFIG),
    )
    .expect("can copy client config from shared cluster");
    std::fs::copy(
        shared_dir.join(SUI_KEYSTORE_FILENAME),
        dst.join(SUI_KEYSTORE_FILENAME),
    )
    .expect("can copy keystore from shared cluster");
    result
}

/// return the path to the `sui` binary that is currently under test
fn get_sui_bin_path() -> String {
    get_cargo_bin("sui")
        .parent()
        .unwrap()
        .to_str()
        .expect("directory name is valid UTF-8")
        .to_owned()
}

/// Return the package dir for the Sui framework packages which may be used in some shell tests.
fn get_sui_package_dir() -> PathBuf {
    let mut path = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
    path.push("../sui-framework/packages");
    path
}

#[cfg(not(msim))]
datatest_stable::harness!(shell_tests, TEST_DIR, TEST_PATTERN);

#[cfg(msim)]
fn main() {}
