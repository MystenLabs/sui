// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

// [test_shell_snapshot] is run on every file matching [TEST_PATTERN] in [TEST_DIR]; this runs the
// files as shell scripts and compares their output to the snapshots; use `cargo insta test
// --review` to update the snapshots.

const TEST_DIR: &str = "tests/tests";
const TEST_PATTERN: &str = r"^test.*\.sh$";

/// run the bash script at [path], comparing its output to the insta snapshot of the same name.
/// The script is run in a temporary working directory that contains a copy of the parent directory
/// of [path], with the `sui-move` binary on the path.
fn test_shell_snapshot(path: &Path) -> datatest_stable::Result<()> {
    // copy files into temporary directory
    let srcdir = path.parent().unwrap();
    let tmpdir = tempfile::tempdir()?;
    let sandbox = tmpdir.path().join("sandbox");

    for entry in WalkDir::new(srcdir) {
        let entry = entry.unwrap();
        let srcfile = entry.path();
        let dstfile = sandbox.join(srcfile.strip_prefix(srcdir)?);
        if srcfile.is_dir() {
            fs::create_dir_all(dstfile)?;
        } else {
            fs::copy(srcfile, dstfile)?;
        }
    }

    // set up path
    // Note: we need to create a symlink instead of just adding the bin dir to the path to prevent
    // local pathnames from leaking into the snapshot files.
    std::os::unix::fs::symlink(
        get_cargo_bin("sui-move").parent().unwrap(),
        tmpdir.path().join("bin"),
    )?;

    // set up command
    let mut shell = Command::new("bash");
    shell
        .env("PATH", "/bin:/usr/bin:../bin")
        .current_dir(sandbox)
        .arg(path.file_name().unwrap());

    // run it!
    let snapshot_name: String = path
        .strip_prefix("tests/tests")?
        .to_string_lossy()
        .to_string();

    assert_cmd_snapshot!(snapshot_name, shell);

    Ok(())
}

datatest_stable::harness!(test_shell_snapshot, TEST_DIR, TEST_PATTERN);
