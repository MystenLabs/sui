// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta_cmd::assert_cmd_snapshot;
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

const TEST_DIR: &str = "tests";
const TEST_PATTERN: &str = r"test.*\.sh";

/// run the bash script at [path], comparing its output to the insta snapshot of the same name.
/// The script is run in a temporary working directory that contains a copy of the parent directory
/// of [path], with the `sui-move` binary on the path.
fn test_shell_snapshot(path: &Path) -> datatest_stable::Result<()> {
    // copy files into temporary directory
    let srcdir = path.parent().unwrap();
    let tmpdir = tempfile::tempdir()?;

    for entry in WalkDir::new(srcdir) {
        let entry = entry.unwrap();
        let srcfile = entry.path();
        let dstfile = tmpdir.path().join(srcfile.strip_prefix(srcdir)?);
        fs::copy(srcfile, dstfile)?;
    }

    // set up command
    let mut shell = Command::new("/bin/bash");
    shell
        .env("PATH", format!("/bin:/usr/bin:{}", cargo_bin_path()))
        .current_dir(tmpdir)
        .arg(path);

    // run it!
    let snapshot_name: String = path.to_string_lossy().to_string();
    assert_cmd_snapshot!(snapshot_name, shell);

    Ok(())
}

/// The parent directory of the `sui-move` binary
fn cargo_bin_path() -> String {
    insta_cmd::get_cargo_bin("sui-move")
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

datatest_stable::harness!(test_shell_snapshot, TEST_DIR, TEST_PATTERN);
