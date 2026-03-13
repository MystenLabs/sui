// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use std::path::Path;

const TEST_DIR: &str = "tests/shell_tests";
const TEST_PATTERN: &str = r"(cli|start)/.*\.sh$";

#[tokio::main]
async fn shell_tests(path: &Path) -> datatest_stable::Result<()> {
    harness::shell_runner::run_shell_script_snapshot(path).await?;
    Ok(())
}

#[cfg(not(msim))]
datatest_stable::harness!(shell_tests, TEST_DIR, TEST_PATTERN);

#[cfg(msim)]
fn main() {}
