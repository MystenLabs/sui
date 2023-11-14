// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
pub const TEST_DIR: &str = "tests";
#[cfg(feature = "pg_integration")]
use sui_transactional_test_runner::run_test;

#[cfg(feature = "pg_integration")]
datatest_stable::harness!(run_test, TEST_DIR, r".*\.(mvir|move)$");
