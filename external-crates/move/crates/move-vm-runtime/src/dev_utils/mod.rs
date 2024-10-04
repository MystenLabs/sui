// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::new_without_default)]

pub(crate) mod dbg_print;
pub mod in_memory_test_adapter;
pub mod storage;
pub mod vm_test_adapter;

#[cfg(not(feature = "tiered-gas"))]
pub mod gas_schedule;

#[cfg(feature = "tiered-gas")]
pub mod tiered_gas_schedule;

#[cfg(feature = "tiered-gas")]
pub use tiered_gas_schedule as gas_schedule;
