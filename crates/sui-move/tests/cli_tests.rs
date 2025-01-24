// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta_cmd::get_cargo_bin;
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

    // set up command
    let mut shell = Command::new("bash");
    shell
        .env("PATH", format!("/bin:/usr/bin:{}", get_sui_move_path()))
        .current_dir(sandbox)
        .arg(path.file_name().unwrap());

    // run it; snapshot test output
    let output = shell.output()?;
    let result = format!(
        "success: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        output.status.success(),
        output.status.code().unwrap_or(!0),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot_name: String = path
        .strip_prefix("tests/tests")?
        .to_string_lossy()
        .to_string();

    insta::with_settings!({description => path.to_string_lossy(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_name, result);
    });

    Ok(())
}

fn get_sui_move_path() -> String {
    get_cargo_bin("sui-move")
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
