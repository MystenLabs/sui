// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod natives_tables;

#[cfg(not(feature = "tiered-gas"))]
pub mod bytecode_based;
#[cfg(not(feature = "tiered-gas"))]
pub use bytecode_based::tables as bytecode_tables;
#[cfg(not(feature = "tiered-gas"))]
pub use bytecode_based::units_types;

#[cfg(feature = "tiered-gas")]
pub mod tier_based;
#[cfg(feature = "tiered-gas")]
pub use tier_based::tables as bytecode_tables;
#[cfg(feature = "tiered-gas")]
pub use tier_based::units_types;
