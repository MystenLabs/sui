// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fs_extra::dir::CopyOptions;
use insta_cmd::get_cargo_bin;
use std::path::{Path, PathBuf};
use std::process::Command;
use sui_config::{Config, SUI_CLIENT_CONFIG, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{FileBasedKeystore, Keystore};
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use tempfile::TempDir;
use test_cluster::TestClusterBuilder;

// [test_shell_snapshot] is run on every file matching [TEST_PATTERN] in [TEST_DIR].
// Files in [TEST_NET_DIR] will be run with a [TestCluster] configured.
//
// These run the files as shell scripts and compares their output to the snapshots; use `cargo
// insta test --review` to update the snapshots.

const TEST_DIR: &str = "tests/shell_tests";
// Temporarily disabled by deleting the folder
const TEST_NET_DIR: &str = "tests/shell_tests/with_network";
const TEST_PATTERN: &str = r"\.sh$";

/// run the bash script at [path], comparing its output to the insta snapshot of the same name.
/// The script is run in a temporary working directory that contains a copy of the parent directory
/// of [path], with the `sui` binary on the path.
///
/// If [cluster] is provided, the config file for the cluster is passed as the `CONFIG` environment
/// variable; otherwise `CONFIG` is set to a temporary file (see [make_temp_config])
#[tokio::main]
async fn test_shell_snapshot(path: &Path) -> datatest_stable::Result<()> {
    // set up test cluster
    let cluster = if path.starts_with(TEST_NET_DIR) {
        Some(TestClusterBuilder::new().build().await)
    } else {
        None
    };

    // copy files into temporary directory
    let srcdir = path.parent().unwrap();
    let tmpdir = tempfile::tempdir()?;
    let sandbox = tmpdir.path();

    let sui_package_dir_src = get_sui_package_dir();

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

    // Note: we create the temporary config file even for cluster tests just so it gets dropped
    let temp_config_dir = make_temp_config_dir();
    let config_file = if let Some(ref cluster) = cluster {
        cluster.swarm.dir()
    } else {
        temp_config_dir.path()
    };
    shell.env("CONFIG", config_file.join(SUI_CLIENT_CONFIG));

    // run it; snapshot test output
    let output = shell.output()?;
    let result = format!(
        "----- script -----\n{}\n----- results -----\nsuccess: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        std::fs::read_to_string(path)?,
        output.status.success(),
        output.status.code().unwrap_or(!0),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot_name: String = path
        .strip_prefix("tests/shell_tests")?
        .to_string_lossy()
        .to_string();

    insta::with_settings!({description => path.to_string_lossy(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_name, result);
    });
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
datatest_stable::harness!(test_shell_snapshot, TEST_DIR, TEST_PATTERN);

#[cfg(msim)]
fn main() {}
