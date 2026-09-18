// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Code from `sui-types` whose correctness is formally verified by Verus.
//!
//! This crate lives at `crates/sui-types/verified/`. The `verus!` macro is
//! a no-op under stable `cargo build`; CI runs `cargo verus check` via
//! scripts/verus-check.sh.
//!
//! To review only the executable code (proof and spec stripped), see the
//! checked-in plain-code artifact `expanded/sui-types-verified.rs`, regenerated
//! by scripts/verus-expand.sh.

pub mod signature_verification;
pub mod utils;
