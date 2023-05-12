// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::new_without_default)]

mod storage;

#[cfg(not(feature = "tiered-gas"))]
pub mod gas_schedule;

#[cfg(feature = "tiered-gas")]
pub mod tiered_gas_schedule;

#[cfg(feature = "tiered-gas")]
pub use tiered_gas_schedule as gas_schedule;

pub use storage::{BlankStorage, DeltaStorage, InMemoryStorage};
