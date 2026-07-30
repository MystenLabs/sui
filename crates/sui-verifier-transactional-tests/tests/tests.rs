// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub const TEST_DIR: &str = "tests";
use sui_transactional_test_runner::run_test;

#[cfg(not(msim))]
datatest_stable::harness!(run_test, TEST_DIR, r".*\.(mvir|move)$");

// These tests drive the bytecode verifier directly, with no network or timing
// behavior for the simulator to perturb, so running them under msim only costs
// time. Expose an empty harness so nextest still sees a well-formed binary.
#[cfg(msim)]
fn main() {
    // Referenced so the otherwise-unused test fn does not trip dead-code warnings.
    let _ = (run_test, TEST_DIR);
    datatest_stable::runner(&[]);
}
