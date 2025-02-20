// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fs_extra::dir::CopyOptions;
use insta_cmd::get_cargo_bin;
use std::fs;
use std::path::Path;
use std::process::Command;
use sui_config::SUI_CLIENT_CONFIG;
use test_cluster::TestClusterBuilder;

// [test_shell_snapshot] is run on every file matching [TEST_PATTERN] in [TEST_DIR].
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
/// If [cluster] is provided, the config file for the cluster is passed as the `CONFIG` environment
/// variable.
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

    fs_extra::dir::copy(srcdir, sandbox, &CopyOptions::new().content_only(true))?;

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

    if let Some(ref cluster) = cluster {
        shell.env("CONFIG", cluster.swarm.dir().join(SUI_CLIENT_CONFIG));
    }

    // run it; snapshot test output
    let output = shell.output()?;
    let result = format!(
        "----- script -----\n{}\n----- results -----\nsuccess: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        fs::read_to_string(path)?,
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

/// return the path to the `sui` binary that is currently under test
fn get_sui_bin_path() -> String {
    get_cargo_bin("sui")
        .parent()
        .unwrap()
        .to_str()
        .expect("directory name is valid UTF-8")
        .to_owned()
}

#[cfg(not(msim))]
datatest_stable::harness!(test_shell_snapshot, TEST_DIR, TEST_PATTERN);

#[cfg(msim)]
fn main() {}
