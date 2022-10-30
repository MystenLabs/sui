// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod compiled_package;

#[cfg(test)]
#[path = "unit_tests/build_tests.rs"]
mod build_tests;

const SUI_PACKAGE_NAME: &str = "Sui";
const MOVE_STDLIB_PACKAGE_NAME: &str = "MoveStdlib";
