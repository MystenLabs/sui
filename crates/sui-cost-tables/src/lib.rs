// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod natives_tables;

#[cfg(not(feature = "tiered-gas"))]
pub mod bytecode_tables;
#[cfg(not(feature = "tiered-gas"))]
pub mod units_types;

#[cfg(feature = "tiered-gas")]
pub mod tiered_tables;
#[cfg(feature = "tiered-gas")]
pub use tiered_tables as bytecode_tables;
#[cfg(feature = "tiered-gas")]
pub mod tiered_units_types;
#[cfg(feature = "tiered-gas")]
pub use tiered_units_types as units_types;
