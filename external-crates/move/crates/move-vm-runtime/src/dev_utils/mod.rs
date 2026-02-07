// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//!  Developer Utilities for writing move tests against the VM, etc.
//!
//!  THE UTILITIES IN THIS DIRECTORY ARE NOT FOR PRODUCTION USE. They are only for writing tests,
//!  and they may use constructs that are not allowed in production code, such as panics, unwraps,
//!  etc.

// These are allowed because dev utilities are only used in test situations in this module, and we
// want to be able to use these constructs to write tests.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
)]


pub(crate) mod dbg_print;
pub mod in_memory_test_adapter;
pub mod storage;
pub mod vm_arguments;
pub mod vm_test_adapter;

#[cfg(test)]
pub mod compilation_utils;

#[cfg(not(feature = "tiered-gas"))]
pub mod gas_schedule;

#[cfg(feature = "tiered-gas")]
pub mod tiered_gas_schedule;

#[cfg(feature = "tiered-gas")]
pub use tiered_gas_schedule as gas_schedule;
