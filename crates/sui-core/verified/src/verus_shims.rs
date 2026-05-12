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

use serde::Serialize;
use shared_crypto::intent::Intent;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfoTrait};
use sui_types::message_envelope::{Envelope, Message};
// AuthoritySignInfo now lives in sui-types-verified (first-class Verus type).
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types_verified::AuthoritySignInfo;
// Spec-only functions — stripped by verus! in stable builds.
#[cfg(verus_only)]
use sui_types_verified::authority_sign_info::{auth_sig_authority_spec, auth_sig_epoch_spec};
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

// AuthoritySignInfo is now defined in sui-types-verified (where it is a
// first-class Verus type). No external_type_specification needed — it is
// owned by this crate's dependency.

// AuthorityQuorumSignInfo still comes from sui-types; register it here.
#[verifier::external_type_specification]
#[verifier::external_body]
#[verifier::reject_recursive_types(STRENGTH)]
pub struct ExAuthorityQuorumSignInfo<const STRENGTH: bool>(AuthorityQuorumSignInfo<STRENGTH>);

// Register Intent so verify_secure can appear in assume_specification.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExIntent(Intent);


// Register serde::Serialize so Verus can accept `T: Serialize` bounds.
// No methods need to be declared — the bound appears only in function
// signatures, not in any spec expression.
#[verifier::external_trait_specification]
pub trait ExSerialize {
    type ExternalTraitSpecificationFor: Serialize;
}

// Register the Message trait so Verus can accept `T: Message` bounds.
// DigestType must be declared because Envelope<T: Message, S> stores
// a T::DigestType in its digest field; Verus needs to know it exists.
#[verifier::external_trait_specification]
pub trait ExMessage {
    type ExternalTraitSpecificationFor: Message;
    type DigestType: Clone + std::fmt::Debug;
}

// Register Envelope generically (T = message payload, S = signature).
#[verifier::external_type_specification]
#[verifier::external_body]
#[verifier::accept_recursive_types(T)]
#[verifier::accept_recursive_types(S)]
pub struct ExEnvelope<T: Message, S>(pub Envelope<T, S>);

// Spec projector: the auth_signature field of an Envelope<T, S>.
// We use a generic S so the assume_specification signature matches the real
// method exactly; callers specialize to S = AuthoritySignInfo as needed.
pub uninterp spec fn envelope_sig_spec<T: Message, S>(e: &Envelope<T, S>) -> S;

pub assume_specification<T: Message, S>[ Envelope::<T, S>::auth_sig ](
    e: &Envelope<T, S>,
) -> (s: &S)
    ensures *s == envelope_sig_spec(e),
;

// Spec for Envelope::into_data_and_sig: the second element of the pair is
// the auth_signature, i.e. the same value as envelope_sig_spec would give.
pub assume_specification<T: Message, S>[ Envelope::<T, S>::into_data_and_sig ](
    e: Envelope<T, S>,
) -> (pair: (T, S))
    ensures pair.1 == envelope_sig_spec(&e),
;

// Convenience projectors for the specialized S = AuthoritySignInfo case.
// These avoid deeply nested calls in postcondition expressions.
pub open spec fn envelope_epoch<T: Message>(e: &Envelope<T, AuthoritySignInfo>) -> u64 {
    auth_sig_epoch_spec(&envelope_sig_spec(e))
}

pub open spec fn envelope_authority<T: Message>(e: &Envelope<T, AuthoritySignInfo>) -> AuthorityName {
    auth_sig_authority_spec(&envelope_sig_spec(e))
}

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
// This is grounded by the assume_specification for verify_secure below:
// sig_is_valid(sig, committee) iff verify_secure returns Ok.
pub uninterp spec fn sig_is_valid(sig: &AuthoritySignInfo, committee: &Committee) -> bool;

// Connect sig_is_valid to the real verify_secure call via a named wrapper.
// verify_secure is defined on the sealed trait AuthoritySignInfoTrait, which
// cannot be registered with external_trait_specification. Instead we expose
// a free function with its own spec. Any proven code that needs to establish
// sig_is_valid should call verify_authority_sig rather than sig.verify_secure.
// The body is trusted (external_body); the spec is the axiom.

#[verifier::external_body]
pub fn verify_authority_sig<T: Message + Serialize>(
    sig: &AuthoritySignInfo,
    data: &T,
    intent: Intent,
    committee: &Committee,
) -> (result: SuiResult<()>)
    ensures result.is_ok() == sig_is_valid(sig, committee),
{
    sig.verify_secure(data, intent, committee)
}


// ---------------------------------------------------------------------------
// Envelope spec projectors (TODO — blocked by Verus limitation)
// ---------------------------------------------------------------------------
//
// To spec `insert`, we need projectors that extract the epoch, authority, and
// validity from an `Envelope<T: Message, AuthoritySignInfo>`.
//
// The canonical fix is to define spec functions in `sui-types` itself (where
// both `Envelope` and `Message` are owned), then either:
//   (a) compile sui-types with `verify = true` so Verus exports the .vir, or
//   (b) wait for cargo-verus to support incremental .vir without full verify.
//
// Attempting (a) currently fails because cargo-verus cannot find the .vir
// file for sui-types when it is listed as a dep of sui-core-verified but not
// included in the CRATES list of verus-check.sh.  Adding it to CRATES fails
// because sui-types with `verify = true` triggers proc-macro compilation
// errors (enum_dispatch etc.) in unrelated modules.
//
// Until this is resolved, the `insert` spec uses `external_body` with the
// intended clauses shown as comments (see stake_aggregator.rs).

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

/// `voted_weight` is monotone: a smaller voted set has no greater weight.
pub proof fn lemma_voted_weight_le_subset(
    c: &Committee,
    s1: Set<AuthorityName>,
    s2: Set<AuthorityName>,
    n: int,
)
    requires
        0 <= n,
        forall|a: AuthorityName| s1.contains(a) ==> #[trigger] s2.contains(a),
    ensures
        voted_weight_le(c, s1, n) <= voted_weight_le(c, s2, n),
    decreases n,
{
    if n <= 0 {
    } else {
        lemma_voted_weight_le_subset(c, s1, s2, n - 1);
        let nm = committee_authorities(c)[n - 1];
        if !s2.contains(nm) {
            // s1 ⊆ s2, so nm ∉ s1 either.
            assert(!s1.contains(nm));
        }
    }
}

pub broadcast proof fn lemma_voted_weight_empty(c: &Committee)
    // Broadcast lemma: Verus pulls this in automatically via the trigger.
    ensures #[trigger] voted_weight(c, Set::<AuthorityName>::empty()) == 0,
{
    lemma_voted_weight_empty_le(c, committee_authorities(c).len() as int);
}

/// For positions strictly above `n`, `weight_of_aux` is unchanged because no
/// later position matches `nm` (they're all different by `committee_unique`).
pub proof fn lemma_weight_of_aux_drops_above(
    c: &Committee,
    nm: AuthorityName,
    n: int,
    k: int,
)
    requires
        0 < n <= k <= committee_authorities(c).len(),
        committee_authorities(c)[n - 1] == nm,
        committee_unique(c),
    ensures
        weight_of_aux(c, nm, k) == weight_of_aux(c, nm, n),
    decreases k,
{
    if k == n {
    } else {
        // Position k-1 ≠ nm by uniqueness (nm is at n-1, n-1 ≠ k-1).
        assert(committee_authorities(c)[k - 1] != nm);
        lemma_weight_of_aux_drops_above(c, nm, n, k - 1);
    }
}

/// `committee_weight_of(c, nm)` equals `committee_weight_seq(c)[n-1]` when
/// nm sits at position `n-1` in the canonical list.
pub proof fn lemma_weight_seq_at_position(c: &Committee, nm: AuthorityName, n: int)
    requires
        0 < n <= committee_authorities(c).len(),
        committee_authorities(c)[n - 1] == nm,
        committee_unique(c),
    ensures
        committee_weight_of(c, nm) == committee_weight_seq(c)[n - 1] as int,
{
    // weight_of_aux(c, nm, n) hits nm at position n-1 by definition.
    assert(weight_of_aux(c, nm, n) == committee_weight_seq(c)[n - 1] as int);
    // Above position n, weight_of_aux is stable (no further matches).
    lemma_weight_of_aux_drops_above(c, nm, n, committee_authorities(c).len() as int);
}

/// In a HashMap iteration sequence, no two positions share the same key.
///
/// Proof: no_duplicates says seq[j1] ≠ seq[j2] for j1 ≠ j2 (as tuples).
/// If seq[j1].0 == seq[j2].0 then from kv_pairs() membership, both carry the
/// same map value, so seq[j1] == seq[j2] — contradiction.
pub proof fn lemma_kv_pairs_key_distinct<K, V>(
    kv_pairs: Seq<(K, V)>,
    m: Map<K, V>,
    j1: int,
    j2: int,
)
    requires
        kv_pairs.no_duplicates(),
        kv_pairs.to_set() =~= m.kv_pairs(),
        0 <= j1 < kv_pairs.len(),
        0 <= j2 < kv_pairs.len(),
        j1 != j2,
    ensures
        kv_pairs[j1].0 != kv_pairs[j2].0,
{
    let pair1 = kv_pairs[j1];
    let pair2 = kv_pairs[j2];
    // no_duplicates: distinct positions → distinct tuples
    assert(pair1 != pair2);
    // coverage: both tuples are in m.kv_pairs()
    assert(m.kv_pairs().contains(pair1));
    assert(m.kv_pairs().contains(pair2));
    // kv_pairs() def: m[pair.0] == pair.1 for each pair
    assert(m[pair1.0] == pair1.1);
    assert(m[pair2.0] == pair2.1);
    // If same key: same value → same tuple, contradicting no_duplicates.
    if pair1.0 == pair2.0 {
        assert(pair1.1 == pair2.1);
        assert(pair1 == pair2);
    }
}

} // verus!
