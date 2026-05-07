// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Code from `sui-types` whose correctness is formally verified by Verus.
//!
//! This crate lives at `crates/sui-types/verified/`. The `verus!` macro is
//! a no-op under stable `cargo build`; CI runs `cargo verus check` via
//! scripts/verus-check.sh.

pub mod authority_name;
pub mod authority_sign_info;
pub mod collections;
pub mod serde_helpers;

// Re-export so downstream verified crates can import directly from here.
pub use authority_name::AuthorityPublicKeyBytes;
pub use authority_sign_info::AuthoritySignInfo;
pub use collections::VerifiedHashMap;
