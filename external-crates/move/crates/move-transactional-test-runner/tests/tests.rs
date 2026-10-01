// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub const TEST_DIR: &str = "tests";
use move_command_line_common::testing::insta::Settings;

fn run_test(path: &std::path::Path) -> datatest_stable::Result<()> {
    let mut settings = Settings::clone_current();
    settings.add_filter(r"(?m)(─ )[^\n]+[/\\]\.tmp[^:\n]+", "$1<TEMP>");
    settings.add_filter(r"(?m)[\t ]+$", "");
    settings.bind(|| move_transactional_test_runner::vm_test_harness::run_test(path))
}

datatest_stable::harness!(run_test, TEST_DIR, r".*\.(mvir|move)$");
