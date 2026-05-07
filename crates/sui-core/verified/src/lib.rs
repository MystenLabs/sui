// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Code from `sui-core` whose correctness is formally verified by Verus.
//!
//! This crate lives at `crates/sui-core/verified/` and holds verified
//! building blocks for use by `sui-core`. The `verus!` macro is a no-op
//! under stable `cargo build`; CI runs `cargo verus check` on this crate
//! to gate merges.

pub mod stake_aggregator;
pub mod verus_shims;
