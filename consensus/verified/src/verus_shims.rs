// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Verus specifications for external types used in this crate.
//!
//! These declarations attach abstract spec functions and pre/post conditions
//! to types and methods defined in `consensus_config`. They have no runtime
//! cost: outside of `cargo verus check`, the `verus!` macro erases everything
//! here to a no-op.
//!
//! The point is to let verified modules in this crate (e.g. `stake_aggregator`)
//! reason about `Committee::stake`, `Committee::reached_quorum`, etc. without
//! needing to modify `consensus_config` itself.

#![allow(unused_imports)]
#![allow(non_local_definitions)]
#![allow(dead_code)]

use consensus_config::{AuthorityIndex, Committee, Stake};
use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// External type specifications
// ---------------------------------------------------------------------------
//
// Tell Verus that these types from `consensus_config` exist. We don't model
// their fields; we only attach behavior via `assume_specification` below.

// Register `AuthorityIndex` with Verus as an opaque external type.
#[verifier::external_type_specification]
// We won't model its layout — only behavior attached via assume_specification.
#[verifier::external_body]
pub struct ExAuthorityIndex(AuthorityIndex);

// Same registration for `Committee`: Verus knows it exists, nothing more.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExCommittee(Committee);

// ---------------------------------------------------------------------------
// Abstract committee model
// ---------------------------------------------------------------------------
//
// These spec functions are uninterpreted: Verus knows they exist and that the
// real `Committee::stake` returns the value indexed by them, but it does not
// know what the values are. That is enough to verify the aggregator's
// invariant.

// Abstract per-authority stake table. Verus reasons about it as an opaque sequence.
pub uninterp spec fn committee_stake_seq(c: &Committee) -> Seq<u64>;
// The quorum threshold (2f+1 in BFT terms). Treated as an unknown u64.
pub uninterp spec fn committee_quorum_spec(c: &Committee) -> u64;
// The validity threshold (f+1). Same treatment as quorum.
pub uninterp spec fn committee_validity_spec(c: &Committee) -> u64;

// Lift `AuthorityIndex` into a mathematical integer for indexing into the stake table.
pub uninterp spec fn authority_index_value_spec(i: AuthorityIndex) -> int;

// Connect AuthorityIndex's exec `value()` to its spec form, and bound it.
pub assume_specification[ AuthorityIndex::value ](i: &AuthorityIndex) -> (v: usize)
    ensures
        // Runtime `value()` agrees with the spec lifting.
        v as int == authority_index_value_spec(*i),
        // AuthorityIndex wraps a u32, so its value always fits in 32 bits.
        v < (1usize << 32),
;

// Specs for the Committee getters we use from verified code.

// Real `Committee::stake(i)` returns the i-th entry of our abstract table.
pub assume_specification[ Committee::stake ](c: &Committee, i: AuthorityIndex) -> (s: Stake)
    ensures
        // Indexing the spec sequence at i gives the stake the runtime returns.
        s == committee_stake_seq(c)[authority_index_value_spec(i)],
;

// `reached_quorum(amt)` is just a comparison against the abstract quorum threshold.
pub assume_specification[ Committee::reached_quorum ](c: &Committee, amt: Stake) -> (b: bool)
    ensures
        // True iff stake meets or exceeds quorum.
        b == (amt >= committee_quorum_spec(c)),
;

// `reached_validity(amt)` is the analogous comparison against the validity threshold.
pub assume_specification[ Committee::reached_validity ](c: &Committee, amt: Stake) -> (b: bool)
    ensures
        // True iff stake meets or exceeds f+1.
        b == (amt >= committee_validity_spec(c)),
;

// `quorum_threshold()` returns the abstract quorum value verbatim.
pub assume_specification[ Committee::quorum_threshold ](c: &Committee) -> (s: Stake)
    ensures
        // Runtime getter agrees with the spec.
        s == committee_quorum_spec(c),
;

// `validity_threshold()` returns the abstract validity value verbatim.
pub assume_specification[ Committee::validity_threshold ](c: &Committee) -> (s: Stake)
    ensures
        // Runtime getter agrees with the spec.
        s == committee_validity_spec(c),
;

} // verus!
