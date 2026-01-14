// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_transactional_test_runner::vm_test_harness::run_test_with_regex_reference_safety;

pub const TEST_DIR: &str = "tests";

datatest_stable::harness!(
    run_test_with_regex_reference_safety,
    TEST_DIR,
    r".*\.(mvir|move)$"
);
