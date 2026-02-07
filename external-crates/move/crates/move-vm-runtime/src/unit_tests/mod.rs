// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// These are allowed because we are in test situations in this module, and we want to be able to
// use these constructs to write tests. In production code, these are not allowed.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    unsafe_code,
)]

mod bad_entry_point_tests;
mod bad_storage_tests;
mod basic_block_tests;
mod binary_format_version;
mod compatibility_tests;
mod exec_func_effects_tests;
mod function_arg_tests;
mod instantiation_tests;
mod interpreter_heap_tests;
mod jit_tests;
mod jump_table_tests;
mod leak_tests;
mod loader_tests;
mod nested_loop_tests;
mod package_cache_tests;
mod publish_tests;
mod return_value_tests;
mod telemetry_tests;
mod value_tests;

#[cfg(all(test, feature = "fuzzing"))]
mod value_prop_tests;
