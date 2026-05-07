// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Verus specifications for external types used by `stake_aggregator`.
//!
//! These declarations attach abstract spec functions and pre/post conditions
//! to types and methods defined in `sui_types`. They have no runtime cost:
//! outside of `cargo verus check`, the `verus!` macro erases everything here
//! to a no-op.

#![allow(unused_imports)]
#![allow(non_local_definitions)]
#![allow(dead_code)]

use shared_crypto::intent::Intent;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait};
use sui_types::error::{SuiError, SuiErrorKind};
use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// External type specifications
// ---------------------------------------------------------------------------
//
// Tell Verus that these types from `sui_types` exist. We don't model their
// fields; we attach behavior via `assume_specification` below.

// AuthorityPublicKeyBytes / AuthorityName are registered and axiomatised in
// sui-types-verified, which is the canonical home. Re-export the broadcast
// lemma so callers in this crate don't need a separate import.
#[cfg(verus_only)]
pub use sui_types_verified::authority_name::axiom_authority_name_key_model;

// Same registration for `Committee`.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExCommittee(Committee);

// SuiError / SuiErrorKind are constructed in error branches.
// Verus only needs to know they exist; we don't reason about their contents.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExSuiError(SuiError);

#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExSuiErrorKind(SuiErrorKind);

// ---------------------------------------------------------------------------
// Cryptographic signature types
// ---------------------------------------------------------------------------

// Register AuthoritySignInfo. Its pub fields (epoch, authority) are accessible
// in spec because the struct itself is registered.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExAuthoritySignInfo(AuthoritySignInfo);

// Register AuthorityQuorumSignInfo — it appears in InsertResult::QuorumReached.
#[verifier::external_type_specification]
#[verifier::external_body]
#[verifier::reject_recursive_types(STRENGTH)]
pub struct ExAuthorityQuorumSignInfo<const STRENGTH: bool>(AuthorityQuorumSignInfo<STRENGTH>);

// Register Intent so verify_secure can appear in assume_specification.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExIntent(Intent);

// Register Envelope generically (T = message payload, S = signature).

// ---------------------------------------------------------------------------
// Cryptographic validity predicate
// ---------------------------------------------------------------------------
//
// `sig_is_valid(sig, committee)` captures whether `sig` is a cryptographically
// correct signature by `sig.authority` against the committee's public key.
//
// The message the sig was made over is intentionally left implicit: in the
// aggregator context all sigs are for the same message, so validity is a
// property of the sig+committee pair once the message context is fixed.
//
// This is an uninterpreted axiom: Verus trusts the outcome of `verify_secure`
// without modelling BLS internals.
pub uninterp spec fn sig_is_valid(sig: &AuthoritySignInfo, committee: &Committee) -> bool;

// ---------------------------------------------------------------------------
// Envelope spec projectors (PARTIALLY IMPLEMENTED — see TODO below)
// ---------------------------------------------------------------------------
//
// The full algebraic spec for `insert` requires spec projectors for the
// Envelope type: envelope_sig_epoch, envelope_sig_authority, envelope_sig_is_valid.
//
// TODO: Registering Envelope<T: Message, S> via external_type_specification
// fails in the current Verus version because the `Message` trait bound is
// opaque to Verus's generic resolution. Once this is resolved (or Envelope
// gains accessor methods in sui-types), add:
//
//   pub uninterp spec fn envelope_sig_epoch<T: Message>(env: &Envelope<T, AuthoritySignInfo>) -> u64;
//   pub uninterp spec fn envelope_sig_authority<T: Message>(env: ...) -> AuthorityName;
//   pub uninterp spec fn envelope_sig_is_valid<T: Message>(env: ..., committee: ...) -> bool;

// TODO: add assume_specification for verify_secure once Envelope registration
// is resolved (see TODO above). The intended spec:
//   result.is_Ok() == sig_is_valid(sig, committee)

// ---------------------------------------------------------------------------
// Abstract committee model
// ---------------------------------------------------------------------------
//
// We model the committee as having a canonical ordered list of authorities
// and a parallel sequence of weights. `committee.weight(name)` returns the
// weight at the position where `name` appears, or 0 if it doesn't.

/// The epoch stored in the committee. Used by `insert` to check epoch-matching.
pub uninterp spec fn committee_epoch_spec(c: &Committee) -> u64;

/// Connect Committee::epoch() to the spec.
pub assume_specification[ Committee::epoch ](c: &Committee) -> (e: u64)
    ensures e == committee_epoch_spec(c),
;

// The committee's authorities, in some canonical order. Real `voting_rights`
// is sorted by AuthorityName, so this corresponds to that sorted view.
pub uninterp spec fn committee_authorities(c: &Committee) -> Seq<AuthorityName>;

// Per-position weight, parallel to `committee_authorities`.
pub uninterp spec fn committee_weight_seq(c: &Committee) -> Seq<u64>;

// Threshold value for a given STRENGTH (true = quorum, false = validity).
pub uninterp spec fn committee_threshold_spec(c: &Committee, strength: bool) -> u64;

// Uniqueness: each authority appears at most once in the canonical list.
// Real `Committee::voting_rights` is sorted by AuthorityName so this holds.
pub open spec fn committee_unique(c: &Committee) -> bool {
    forall|i: int, j: int|
        0 <= i < committee_authorities(c).len()
        && 0 <= j < committee_authorities(c).len()
        && i != j
        ==> committee_authorities(c)[i] != committee_authorities(c)[j]
}

// `committee.weight(name)` returns the weight at the position where `name`
// appears, walking from the end of the canonical list. With uniqueness,
// this is the only match.
pub open spec fn committee_weight_of(c: &Committee, name: AuthorityName) -> int
    decreases committee_authorities(c).len(),
{
    weight_of_aux(c, name, committee_authorities(c).len() as int)
}

pub open spec fn weight_of_aux(c: &Committee, name: AuthorityName, n: int) -> int
    decreases n,
{
    if n <= 0 {
        0
    } else if committee_authorities(c)[n - 1] == name {
        committee_weight_seq(c)[n - 1] as int
    } else {
        weight_of_aux(c, name, n - 1)
    }
}

// `Committee::weight(name)` returns the abstract per-name weight. This relies
// on the real `weight()` doing a binary_search over `voting_rights` which is
// sorted by name; the canonical-list view above corresponds to that sorted
// order.
pub assume_specification[ <Committee as sui_types::committee::CommitteeTrait<AuthorityName>>::weight ](
    c: &Committee,
    name: &AuthorityName,
) -> (w: StakeUnit)
    ensures
        // Runtime weight matches the spec walk.
        w as int == committee_weight_of(c, *name),
;

// `Committee::threshold::<STRENGTH>()` returns the abstract threshold for
// the given STRENGTH bit (true = quorum 2f+1, false = validity f+1).
pub assume_specification<const STRENGTH: bool>[ Committee::threshold::<STRENGTH> ](
    c: &Committee,
) -> (t: StakeUnit)
    ensures
        // Whichever branch is selected matches the abstract spec.
        t == committee_threshold_spec(c, STRENGTH),
;

// ---------------------------------------------------------------------------
// Sum-of-weights spec + lemmas
// ---------------------------------------------------------------------------
//
// `voted_weight(c, voted)` totals committee.weight(name) for names in
// `voted`, by walking the canonical committee list. Names NOT in the
// committee contribute 0. This is the load-bearing spec for any future
// proof of `StakeAggregator::insert_generic`'s sum invariant.

pub open spec fn voted_weight_le(c: &Committee, voted: Set<AuthorityName>, n: int) -> int
    // Recursion on the prefix length n; matches the canonical list 0..n.
    decreases n,
{
    // Empty prefix: total is 0.
    if n <= 0 {
        0
    } else {
        // Look at position n-1 in the canonical list.
        let nm = committee_authorities(c)[n - 1];
        if voted.contains(nm) {
            // Counted: add its weight, recurse on shorter prefix.
            committee_weight_seq(c)[n - 1] as int + voted_weight_le(c, voted, n - 1)
        } else {
            // Not voted: skip this position.
            voted_weight_le(c, voted, n - 1)
        }
    }
}

pub open spec fn voted_weight(c: &Committee, voted: Set<AuthorityName>) -> int {
    // Total over the full canonical list.
    voted_weight_le(c, voted, committee_authorities(c).len() as int)
}

/// Inserting an authority adds its weight to the partial sum (or 0 if the
/// authority is not in the canonical list — i.e., not a real committee member).
pub proof fn lemma_voted_weight_insert_le(
    c: &Committee,
    voted: Set<AuthorityName>,
    name: AuthorityName,
    n: int,
)
    requires
        // Walking a valid prefix of the canonical list.
        0 <= n <= committee_authorities(c).len(),
        // Inserted name is genuinely new in `voted`.
        !voted.contains(name),
        // Canonical list has no duplicate names.
        committee_unique(c),
    ensures
        voted_weight_le(c, voted.insert(name), n)
            == voted_weight_le(c, voted, n) + weight_of_aux(c, name, n),
    // Induction over the prefix length.
    decreases n,
{
    if n <= 0 {
        // Base case: both sides are 0.
    } else {
        // Inductive case: peel off position n-1 and recurse.
        lemma_voted_weight_insert_le(c, voted, name, n - 1);
        // When position n-1 holds the inserted name, uniqueness gives
        // weight_of_aux(c, name, n-1) == 0 (name doesn't appear earlier).
        if committee_authorities(c)[n - 1] == name {
            assert(weight_of_aux(c, name, n - 1) == 0) by {
                // Side-lemma: walk earlier positions, none of which match `name`.
                lemma_weight_of_aux_zero_when_unique_at_n(c, name, n - 1, n - 1);
            };
        }
    }
}

/// If `name` appears at position `pos` (with uniqueness), then
/// `weight_of_aux(c, name, n) == 0` for any `n <= pos`.
pub proof fn lemma_weight_of_aux_zero_when_unique_at_n(
    c: &Committee,
    name: AuthorityName,
    n: int,
    pos: int,
)
    requires
        // Canonical list uniqueness.
        committee_unique(c),
        // `pos` is the position where `name` appears.
        0 <= pos < committee_authorities(c).len(),
        committee_authorities(c)[pos] == name,
        // Walking only a prefix that ends at or before `pos`.
        0 <= n <= pos,
    ensures
        weight_of_aux(c, name, n) == 0,
    // Induction over n.
    decreases n,
{
    if n <= 0 {
    } else {
        // Position n-1 cannot equal `name` (uniqueness, since name is at pos).
        lemma_weight_of_aux_zero_when_unique_at_n(c, name, n - 1, pos);
    }
}

/// Specialization of the prefix lemma to the full canonical length.
pub proof fn lemma_voted_weight_insert(
    c: &Committee,
    voted: Set<AuthorityName>,
    name: AuthorityName,
)
    requires
        // Inserted name is new.
        !voted.contains(name),
        // Canonical list uniqueness.
        committee_unique(c),
    ensures
        voted_weight(c, voted.insert(name))
            == voted_weight(c, voted) + committee_weight_of(c, name),
{
    // Apply the prefix lemma at the full canonical length.
    lemma_voted_weight_insert_le(c, voted, name, committee_authorities(c).len() as int);
}

/// Empty voter set has zero total weight at every prefix length.
pub proof fn lemma_voted_weight_empty_le(c: &Committee, n: int)
    // n must be non-negative; matches recursion guard.
    requires 0 <= n,
    ensures voted_weight_le(c, Set::<AuthorityName>::empty(), n) == 0,
    // Induction over n.
    decreases n,
{
    if n <= 0 {
    } else {
        lemma_voted_weight_empty_le(c, n - 1);
    }
}

pub broadcast proof fn lemma_voted_weight_empty(c: &Committee)
    // Broadcast lemma: Verus pulls this in automatically via the trigger.
    ensures #[trigger] voted_weight(c, Set::<AuthorityName>::empty()) == 0,
{
    lemma_voted_weight_empty_le(c, committee_authorities(c).len() as int);
}

} // verus!
