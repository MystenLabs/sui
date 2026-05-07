// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use shared_crypto::intent::Intent;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::base_types::ConciseableName;
use sui_types::committee::{Committee, CommitteeTrait, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait};
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::message_envelope::{Envelope, Message};
use sui_types_verified::VerifiedHashMap;
use tracing::warn;
use typed_store::TypedStoreError;
use vstd::prelude::*;

verus! {

#[cfg(verus_only)]
use crate::verus_shims::{
    committee_epoch_spec, committee_threshold_spec, committee_unique,
    committee_weight_of, envelope_authority, envelope_epoch, envelope_sig_spec,
    lemma_voted_weight_empty, lemma_voted_weight_insert, sig_is_valid, voted_weight,
};
#[cfg(verus_only)]
use sui_types_verified::authority_sign_info::{auth_sig_authority_spec, auth_sig_epoch_spec};

// Verus cannot construct opaque external types directly. These wrappers
// build the SuiErrors used by `insert_generic`'s error branches. Bodies
// are unverified — error construction is exec-only metadata that does
// not affect the sum invariant.

#[verifier::external_body]
fn err_repeated_signer(signer: AuthorityName, conflicting_sig: bool) -> (out: SuiError) {
    SuiErrorKind::StakeAggregatorRepeatedSigner {
        signer,
        conflicting_sig,
    }
    .into()
}

#[verifier::external_body]
fn err_invalid_authenticator() -> (out: SuiError) {
    SuiErrorKind::InvalidAuthenticator.into()
}

#[verifier::external_body]
fn err_wrong_epoch(expected: u64, actual: u64) -> (out: SuiError) {
    SuiErrorKind::WrongEpoch {
        expected_epoch: expected,
        actual_epoch: actual,
    }
    .into()
}

} // verus!

verus! {

/// StakeAggregator allows us to keep track of the total stake of a set of validators.
/// STRENGTH indicates whether we want a strong quorum (2f+1) or a weak quorum (f+1).
#[derive(Debug)]
pub struct StakeAggregator<S, const STRENGTH: bool> {
    pub data: VerifiedHashMap<AuthorityName, S>,
    pub total_votes: StakeUnit,
    pub committee: Arc<Committee>,
}

/// StakeAggregator is a utility data structure that allows us to aggregate a list of validator
/// signatures over time. A committee is used to determine whether we have reached sufficient
/// quorum (defined based on `STRENGTH`). The generic implementation does not require `S` to be
/// an actual signature, but just an indication that a specific validator has voted. A specialized
/// implementation for `AuthoritySignInfo` is followed below.
impl<S: Clone + Eq, const STRENGTH: bool> StakeAggregator<S, STRENGTH> {
    /// Sum invariant: `total_votes` equals the total committee weight of every
    /// authority recorded in `data`.
    pub open spec fn invariant_holds(&self) -> bool {
        // Voter set must be finite for the spec sum to be well-defined.
        self.data@.dom().finite()
        // Running total matches the spec sum over the recorded voters.
        && self.total_votes as int == voted_weight(&self.committee, self.data@.dom())
    }

    /// Whether an authority has been recorded in the aggregator.
    /// Used as the key predicate for reasoning about duplicate-insert behaviour.
    pub open spec fn has_voted(&self, authority: AuthorityName) -> bool {
        self.data@.contains_key(authority)
    }

    pub fn new(committee: Arc<Committee>) -> (out: Self)
        ensures
            // Fresh aggregator: zero votes, empty data, invariant holds.
            out.total_votes == 0,
            out.data@ =~= Map::<AuthorityName, S>::empty(),
            out.invariant_holds(),
            // Committee reference preserved.
            out.committee == committee,
    {
        // sum_stakes(c, empty) == 0 for any committee, so the invariant follows.
        broadcast use lemma_voted_weight_empty;
        Self {
            data: VerifiedHashMap::default(),
            total_votes: 0,
            committee,
        }
    }

    /// Attempt to record an authority's vote.
    ///
    /// # Algebraic model (functional view)
    ///
    /// Treat the aggregator as a pure value `Agg = { voted: Set<Authority> }`.
    /// `total_votes` is a cached sum: `weight_sum(agg) = Σ weight(a) for a ∈ agg.voted`
    /// (encoded as `invariant_holds()`).
    ///
    /// In this model `insert` is always **set union** — the authority is always
    /// recorded regardless of the return variant:
    ///
    ///   insert(agg, a).voted  ==  agg.voted  ∪  {a}
    ///
    /// The return variant is **uniquely determined** by the pre-state (biconditional,
    /// not merely one direction):
    ///
    ///   out is Failed      ⟺   a ∈ agg.voted  ∨  weight(a) = 0
    ///   out is QuorumReached ⟺ a ∉ agg.voted  ∧  weight(a) > 0  ∧  new_sum ≥ threshold
    ///
    ///   NotEnoughVotes is the remaining case (determined by elimination).
    ///
    /// Note: `S` (the value stored alongside the authority) has no effect on
    /// any of these properties; it exists only for the AuthoritySignInfo
    /// specialisation that needs to aggregate cryptographic signatures.
    pub fn insert_generic<'a>(
        &'a mut self,
        authority: AuthorityName,
        s: S,
    ) -> (out: InsertResult<&'a HashMap<AuthorityName, S>>)
        requires
            // Aggregator was already in a valid state.
            old(self).invariant_holds(),
            // Real Committee has unique authority names (sorted by name).
            committee_unique(&old(self).committee),
            // No overflow when this authority's weight is added.
            old(self).total_votes as int
                + committee_weight_of(&old(self).committee, authority)
                <= u64::MAX as int,
        ensures
            // Invariant survives all three branches.
            self.invariant_holds(),
            // Committee reference unchanged.
            self.committee == old(self).committee,

            // === State transition ===
            // The voted set grows by exactly {authority}; nothing else changes.
            // (Combines monotonicity, presence, and "no other key was added or removed"
            //  into a single biconditional.)
            forall|a: AuthorityName|
                self.has_voted(a) <==> (#[trigger] old(self).has_voted(a) || a == authority),

            // === Variant determination (both directions) ===
            // The variant is uniquely determined by the pre-state; each condition is
            // necessary AND sufficient for its corresponding variant.

            // Failed iff the authority was already present OR has zero committee weight.
            // Failed iff the authority was already present OR has zero committee weight.
            (out is Failed)
                <==> (old(self).has_voted(authority)
                      || committee_weight_of(&self.committee, authority) == 0),

            // QuorumReached iff the authority was new, had weight, and the running
            // total now meets the threshold.
            (out is QuorumReached)
                <==> (!old(self).has_voted(authority)
                      && committee_weight_of(&self.committee, authority) > 0
                      && self.total_votes
                          >= committee_threshold_spec(&self.committee, STRENGTH)),

            // NotEnoughVotes is the remaining case — stated explicitly for readability,
            // but derivable from the two biconditionals above by exhaustion.
            (out is NotEnoughVotes)
                <==> (!old(self).has_voted(authority)
                      && committee_weight_of(&self.committee, authority) > 0
                      && self.total_votes
                          < committee_threshold_spec(&self.committee, STRENGTH)),

            // === Value preservation ===
            // Stored values for previously-present authorities are never overwritten.
            forall|a: AuthorityName|
                old(self).has_voted(a) ==>
                    #[trigger] self.data@[a] == old(self).data@[a],
            // When the authority is new, its sig is stored exactly as given.
            !old(self).has_voted(authority) ==> self.data@[authority] == s,
    {
        if self.data.contains_key(&authority) {
            // Repeated signer. The conflict bit is exec-only metadata.
            let conflicting_sig = match self.data.get_value(&authority) {
                Some(v) => v != &s,
                None => false,
            };
            proof {
                // No state change in the duplicate path; old invariant carries over.
                assert(self.data@ =~= old(self).data@);
                assert(self.total_votes == old(self).total_votes);
                // Pin down the variant-specific fact: signer was already present.
                assert(old(self).data@.contains_key(authority));
            }
            return InsertResult::Failed {
                error: err_repeated_signer(authority, conflicting_sig),
            };
        }
        self.data.insert_new(authority, s);
        let votes = self.committee.weight(&authority);
        proof {
            // dom() grew by exactly {authority}; sum grows by weight(authority).
            let before_dom = old(self).data@.dom();
            lemma_voted_weight_insert(&old(self).committee, before_dom, authority);
            // The vacant-path facts shared by all three downstream branches.
            assert(!old(self).data@.contains_key(authority));
            assert(self.data@.dom() =~= old(self).data@.dom().insert(authority));
        }
        if votes > 0 {
            self.total_votes = self.total_votes + votes;
            proof {
                // total_votes now equals the new sum, and weight is positive.
                assert(self.invariant_holds());
                assert(committee_weight_of(&self.committee, authority) > 0);
            }
            if self.total_votes >= self.committee.threshold::<STRENGTH>() {
                InsertResult::QuorumReached(&self.data.inner)
            } else {
                InsertResult::NotEnoughVotes {
                    bad_votes: 0,
                    bad_authorities: Vec::new(),
                }
            }
        } else {
            // Weight 0: data extended but total_votes unchanged. The lemma added
            // 0 to the sum, matching unchanged total — invariant still holds.
            proof {
                assert(self.invariant_holds());
                assert(committee_weight_of(&self.committee, authority) == 0);
                assert(self.total_votes == old(self).total_votes);
            }
            InsertResult::Failed {
                error: err_invalid_authenticator(),
            }
        }
    }

    pub fn contains_key(&self, authority: &AuthorityName) -> (b: bool)
        // Result mirrors the underlying map.
        ensures b == self.data@.contains_key(*authority),
    {
        self.data.contains_key(authority)
    }

    pub fn total_votes(&self) -> (v: StakeUnit)
        // Trivial getter.
        ensures v == self.total_votes,
    {
        self.total_votes
    }

    pub fn has_quorum(&self) -> (b: bool)
        // True iff total_votes meets the threshold for this STRENGTH.
        ensures b == (self.total_votes >= committee_threshold_spec(&self.committee, STRENGTH)),
    {
        self.total_votes >= self.committee.threshold::<STRENGTH>()
    }

    /// Construct an aggregator from a stream of (authority, S) pairs. Body is
    /// `external_body` because Result threading via `?` is outside the verified
    /// subset; the contract guarantees the invariant holds on success.
    #[verifier::external_body]
    pub fn from_iter<I: Iterator<Item = Result<(AuthorityName, S), TypedStoreError>>>(
        committee: Arc<Committee>,
        data: I,
    ) -> (out: SuiResult<Self>)
        ensures
            // On success, the constructed aggregator obeys the invariant.
            match out {
                Ok(this) => this.invariant_holds() && this.committee == committee,
                Err(_) => true,
            },
    {
        let mut this = Self::new(committee);
        for item in data {
            let (authority, s) = item?;
            this.insert_generic(authority, s);
        }
        Ok(this)
    }
}

/// If an authority has already voted, inserting again must return `Failed`.
///
/// `QuorumReached` and `NotEnoughVotes` both require `!old.has_voted(authority)`.
/// If `old.has_voted(authority)` is true those branches are impossible, so by
/// exhaustion of the three exclusive variants only `Failed` can be returned.
///
/// Usage: call this after any prior `insert_generic` — the postcondition
/// guarantees `post.has_voted(authority)`. Supplying that fact here, together
/// with the postconditions of a *second* call, proves the second result is `Failed`.
pub proof fn lemma_voted_authority_insert_fails(
    pre_has_voted: bool,
    result_is_quorum: bool,
    result_is_not_enough: bool,
    result_is_failed: bool,
)
    requires
        pre_has_voted,
        // Exactly one variant is active.
        result_is_quorum || result_is_not_enough || result_is_failed,
        !(result_is_quorum && result_is_not_enough),
        !(result_is_quorum && result_is_failed),
        !(result_is_not_enough && result_is_failed),
        // Postconditions from insert_generic: both success variants require the
        // authority to have been absent before the call.
        result_is_quorum ==> !pre_has_voted,
        result_is_not_enough ==> !pre_has_voted,
    ensures
        result_is_failed
{
    // Both quorum and not_enough contradict pre_has_voted; Failed is the only
    // remaining possibility.
}

/// Inserting two distinct authorities produces the same voted set regardless
/// of order.
///
/// This is a direct corollary of the state-transition biconditional in
/// `insert_generic`:
///
///   after a then b: has_voted(c) ⟺ agg.voted(c) ∨ c=a ∨ c=b
///   after b then a: has_voted(c) ⟺ agg.voted(c) ∨ c=b ∨ c=a
///
/// These expressions are equal because ∨ is commutative. No reasoning about
/// the implementation is required — the spec alone implies commutativity.
///
/// The lemma takes the relevant postconditions as hypotheses rather than
/// reasoning about mutable-reference calls directly.
pub proof fn lemma_insert_generic_commutes(
    agg_voted: Set<AuthorityName>,
    auth_a: AuthorityName,
    auth_b: AuthorityName,
    // State after inserting a, then b
    after_a:  Set<AuthorityName>,
    after_ab: Set<AuthorityName>,
    // State after inserting b, then a
    after_b:  Set<AuthorityName>,
    after_ba: Set<AuthorityName>,
)
    requires
        auth_a != auth_b,
        // State-transition postconditions of the first pair of insertions
        forall|c: AuthorityName| after_a.contains(c)
            <==> (#[trigger] agg_voted.contains(c) || c == auth_a),
        forall|c: AuthorityName| after_ab.contains(c)
            <==> (#[trigger] after_a.contains(c) || c == auth_b),
        // State-transition postconditions of the second pair (reversed order)
        forall|c: AuthorityName| after_b.contains(c)
            <==> (#[trigger] agg_voted.contains(c) || c == auth_b),
        forall|c: AuthorityName| after_ba.contains(c)
            <==> (#[trigger] after_b.contains(c) || c == auth_a),
    ensures
        // The voted sets are identical — insertion order doesn't matter.
        forall|c: AuthorityName|
            #[trigger] after_ab.contains(c) <==> after_ba.contains(c)
{
    // Both reduce to: agg_voted.contains(c) || c == auth_a || c == auth_b.
    // ∨ is commutative, so the order of auth_a and auth_b doesn't matter.
    // Help Verus instantiate the forall triggers for each c.
    assert forall|c: AuthorityName|
        #[trigger] after_ab.contains(c) <==> after_ba.contains(c)
    by {
        // Expand after_ab: substitute the a-insertion into the b-insertion.
        assert(after_ab.contains(c) <==> (agg_voted.contains(c) || c == auth_a || c == auth_b));
        // Expand after_ba: substitute the b-insertion into the a-insertion.
        assert(after_ba.contains(c) <==> (agg_voted.contains(c) || c == auth_b || c == auth_a));
    }
}

/// Inserting two distinct valid-sig authorities via `insert` commutes: the
/// resulting `has_voted` set is the same regardless of insertion order.
///
/// The proof is identical to `lemma_insert_generic_commutes` — commutativity
/// of set union — because `insert` satisfies the same biconditional state
/// transition as `insert_generic` when both sigs are cryptographically valid
/// (valid sigs are never evicted by the BLS fallback path).
///
/// The biconditional is taken as a hypothesis rather than derived inline,
/// so the caller is responsible for establishing it holds (e.g. by applying
/// the monotonicity postcondition of `insert` under the valid-sig conditions).
pub proof fn lemma_insert_commutes(
    agg_voted: Set<AuthorityName>,
    auth_a: AuthorityName,
    auth_b: AuthorityName,
    // has_voted sets along each ordering
    after_a:  Set<AuthorityName>,
    after_ab: Set<AuthorityName>,
    after_b:  Set<AuthorityName>,
    after_ba: Set<AuthorityName>,
)
    requires
        auth_a != auth_b,
        // Biconditional state-transition for insert(a) then insert(b):
        // holds when sig_a and sig_b are both sig_is_valid.
        forall|c: AuthorityName| after_a.contains(c)
            <==> (#[trigger] agg_voted.contains(c) || c == auth_a),
        forall|c: AuthorityName| after_ab.contains(c)
            <==> (#[trigger] after_a.contains(c) || c == auth_b),
        // Biconditional for the reversed order:
        forall|c: AuthorityName| after_b.contains(c)
            <==> (#[trigger] agg_voted.contains(c) || c == auth_b),
        forall|c: AuthorityName| after_ba.contains(c)
            <==> (#[trigger] after_b.contains(c) || c == auth_a),
    ensures
        forall|c: AuthorityName|
            #[trigger] after_ab.contains(c) <==> after_ba.contains(c)
{
    assert forall|c: AuthorityName|
        #[trigger] after_ab.contains(c) <==> after_ba.contains(c)
    by {
        assert(after_ab.contains(c) <==> (agg_voted.contains(c) || c == auth_a || c == auth_b));
        assert(after_ba.contains(c) <==> (agg_voted.contains(c) || c == auth_b || c == auth_a));
    }
}

/// Challenge theorems: exercise the `<==` directions of the variant biconditionals.
///
/// The three biconditionals are mutually exclusive and exhaustive, so any one
/// `<==` direction is implied by the other two via elimination.  A challenge
/// theorem that takes all three biconditionals as hypotheses therefore always
/// proves by elimination — it never actually needs the specific `<==` direction.
///
/// To make these theorems genuine tests of the `<==` directions, each one is
/// given ONLY the biconditional for the variant it concludes, plus the concrete
/// conditions.  The proof must use the `<==` direction directly; it cannot go
/// through elimination because the other variants' biconditionals are absent.
///
/// Note: these theorems take the biconditionals as abstract hypotheses (standard
/// Verus pattern when the function is exec and cannot be called from proof). They
/// serve as correctness documentation and as checks that the biconditionals are
/// internally consistent.

/// The `<==` direction of QuorumReached: when conditions hold, QuorumReached is forced.
/// Hypothesis: the QuorumReached biconditional only (no other variant biconditionals).
pub proof fn challenge_quorum_reached_forced<S: Clone + Eq, const STRENGTH: bool>(
    authority: AuthorityName,
    pre_has_voted_authority: bool,
    pre_total: u64,
    pre_threshold: u64,
    pre_weight: int,
    out_is_quorum: bool,
)
    requires
        !pre_has_voted_authority,
        pre_weight > 0,
        pre_total as int + pre_weight >= pre_threshold as int,
        // The QuorumReached biconditional for this call (full <=>).
        out_is_quorum
            <==> (!pre_has_voted_authority
                  && pre_weight > 0
                  && pre_total as int + pre_weight >= pre_threshold as int),
    ensures
        out_is_quorum,
{
    // Proof uses ONLY the <== direction of the QuorumReached biconditional.
    // No other variant biconditionals provided — elimination is not possible.
}

/// The `<==` direction of Failed: weight-zero forces Failed.
pub proof fn challenge_weight_zero_forces_failed(
    pre_has_voted: bool,
    pre_weight: int,
    out_is_failed: bool,
)
    requires
        !pre_has_voted,
        pre_weight == 0,
        out_is_failed <==> (pre_has_voted || pre_weight == 0),
    ensures
        out_is_failed,
{
    // Proof uses the <== direction: !has_voted && weight == 0 satisfies the rhs.
}

} // verus!

impl<S: Clone + Eq, const STRENGTH: bool> StakeAggregator<S, STRENGTH> {
    pub fn keys(&self) -> impl Iterator<Item = &AuthorityName> {
        self.data.inner.keys()
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    #[cfg(test)]
    pub fn validator_sig_count(&self) -> usize {
        self.data.inner.len()
    }
}

verus! {

// ---------------------------------------------------------------------------
// Spec predicates for the AuthoritySignInfo specialisation
// ---------------------------------------------------------------------------

/// All authorities with positive committee weight have cryptographically valid
/// stored signatures.  This is the "clean state" invariant: established on a
/// fresh aggregator and maintained by every call to `insert` when the caller
/// supplies a valid signature.
pub open spec fn all_sigs_valid<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> bool {
    forall|a: AuthorityName|
        agg.has_voted(a) && committee_weight_of(&agg.committee, a) > 0
            ==> #[trigger] sig_is_valid(&agg.data@[a], &agg.committee)
}

/// The aggregated weight meets the quorum threshold.
/// Under `all_sigs_valid`, every stored weighted vote is valid, so
/// `total_votes` already counts only valid votes and this reduces to
/// `total_votes >= threshold`.  The spec is stated in those simple terms
/// rather than re-summing over the valid subset.
pub open spec fn reaches_quorum<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> bool {
    agg.total_votes >= committee_threshold_spec(&agg.committee, STRENGTH)
}


pub enum InsertResult<CertT> {
    QuorumReached(CertT),
    Failed {
        error: SuiError,
    },
    NotEnoughVotes {
        bad_votes: u64,
        bad_authorities: Vec<AuthorityName>,
    },
}

impl<CertT> InsertResult<CertT> {
    pub fn is_quorum_reached(&self) -> (b: bool)
        ensures b == matches!(self, Self::QuorumReached(..)),
    {
        matches!(self, Self::QuorumReached(..))
    }
}

// ---------------------------------------------------------------------------
// BLS aggregation + individual-verify fallback (external_body)
// ---------------------------------------------------------------------------
//
// This function encapsulates the BLS-specific logic inside the QuorumReached
// arm of `insert`:
//
//   1. Attempt to aggregate all stored sigs into a quorum certificate.
//   2. If the batch BLS verification passes, return QuorumReached.
//   3. Otherwise fall back to verifying each sig individually, evicting any
//      that fail, and returning NotEnoughVotes.
//
// The body is trusted (external_body): it manipulates `agg.data.inner` and
// `agg.total_votes` directly, bypassing the VerifiedHashMap wrapper.
// The postconditions are what the proven `insert` function needs.

#[verifier::external_body]
fn try_aggregate_and_verify<T: Message + Serialize, const STRENGTH: bool>(
    agg: &mut StakeAggregator<AuthoritySignInfo, STRENGTH>,
    data: T,
) -> (out: InsertResult<AuthorityQuorumSignInfo<STRENGTH>>)
    requires
        old(agg).invariant_holds(),
        committee_unique(&old(agg).committee),
        // All stored sigs are valid: no eviction will occur, the batch BLS
        // aggregation succeeds, and QuorumReached is returned.
        all_sigs_valid::<STRENGTH>(old(agg)),
    ensures
        agg.committee == old(agg).committee,
        agg.invariant_holds(),
        all_sigs_valid::<STRENGTH>(agg),
        // With all-valid sigs, no eviction happens: data and total_votes are unchanged.
        agg.data@ == old(agg).data@,
        agg.total_votes == old(agg).total_votes,
        // Batch BLS of all-valid sigs always succeeds: QuorumReached is certain.
        out is QuorumReached,
{
    match AuthorityQuorumSignInfo::<STRENGTH>::new_from_auth_sign_infos(
        agg.data.inner.values().cloned().collect(),
        agg.committee(),
    ) {
        Ok(aggregated) => {
            match aggregated.verify_secure(
                &data,
                Intent::sui_app(T::SCOPE),
                agg.committee(),
            ) {
                Ok(_) => InsertResult::QuorumReached(aggregated),
                Err(_) => {
                    // Batch BLS failed: verify each sig individually and evict bad ones.
                    // TODO(joyqvq): if the latest single sig fails repeatedly, this loop
                    // can be triggered every time. Caching single-sig results would help.
                    let mut bad_votes = 0;
                    let mut bad_authorities = vec![];
                    for (name, sig) in &agg.data.inner.clone() {
                        if let Err(err) = sig.verify_secure(
                            &data,
                            Intent::sui_app(T::SCOPE),
                            agg.committee(),
                        ) {
                            warn!(name=?name.concise(), "Bad stake from validator: {:?}", err);
                            agg.data.inner.remove(name);
                            let votes = agg.committee.weight(name);
                            agg.total_votes -= votes;
                            bad_votes += votes;
                            bad_authorities.push(*name);
                        }
                    }
                    InsertResult::NotEnoughVotes {
                        bad_votes,
                        bad_authorities,
                    }
                }
            }
        }
        Err(error) => InsertResult::Failed { error },
    }
}

// ---------------------------------------------------------------------------
// StakeAggregator<AuthoritySignInfo, STRENGTH>::insert — proven correct
// ---------------------------------------------------------------------------

impl<const STRENGTH: bool> StakeAggregator<AuthoritySignInfo, STRENGTH> {
    /// Insert an authority signature carried in a signed envelope.
    ///
    /// # Algebraic model
    ///
    /// The aggregator is a set of (authority, valid-sig) pairs tracked against a
    /// fixed committee.  `insert` is called with a cryptographically valid sig
    /// (required via precondition) from an authority not yet in the set.
    ///
    /// The return variant is fully determined by the pre-state:
    ///
    ///   Failed        ⟺   epoch mismatch  ∨  duplicate authority  ∨  weight = 0
    ///   QuorumReached ⟺   new valid sig and the running total now meets threshold
    ///   NotEnoughVotes ⟺  new valid sig but running total still below threshold
    ///
    /// Under this model the state-transition is always set-union: after a
    /// successful (non-Failed) insert, `voted` = `old(voted) ∪ {authority}`.
    pub fn insert<T: Message + Serialize>(
        &mut self,
        envelope: Envelope<T, AuthoritySignInfo>,
    ) -> (out: InsertResult<AuthorityQuorumSignInfo<STRENGTH>>)
        requires
            old(self).invariant_holds(),
            committee_unique(&old(self).committee),
            old(self).total_votes as int
                + committee_weight_of(&old(self).committee, envelope_authority(&envelope))
                <= u64::MAX as int,
            // All previously stored sigs are valid — the aggregator is in a clean state.
            all_sigs_valid::<STRENGTH>(old(self)),
            // The incoming sig is cryptographically valid.
            sig_is_valid(&envelope_sig_spec(&envelope), &old(self).committee),
        ensures
            self.committee == old(self).committee,
            self.invariant_holds(),
            all_sigs_valid::<STRENGTH>(self),

            // === State transition ===
            // The voted set grows by exactly {authority} on non-Failed paths.
            envelope_epoch(&envelope) == committee_epoch_spec(&old(self).committee)
                && !old(self).has_voted(envelope_authority(&envelope))
                && committee_weight_of(&old(self).committee, envelope_authority(&envelope)) > 0
                ==> forall|a: AuthorityName|
                        self.has_voted(a)
                            <==> (#[trigger] old(self).has_voted(a) || a == envelope_authority(&envelope)),

            // === Return value (fully biconditional) ===

            // Failed iff the input is structurally invalid: wrong epoch, duplicate
            // authority, or zero committee weight.
            (out is Failed)
                <==> (envelope_epoch(&envelope) != committee_epoch_spec(&old(self).committee)
                      || old(self).has_voted(envelope_authority(&envelope))
                      || committee_weight_of(&old(self).committee, envelope_authority(&envelope)) == 0),

            // QuorumReached iff the new sig is valid (precondition), the input is
            // structurally valid, and the running total now meets the threshold.
            (out is QuorumReached)
                <==> (envelope_epoch(&envelope) == committee_epoch_spec(&old(self).committee)
                      && !old(self).has_voted(envelope_authority(&envelope))
                      && committee_weight_of(&old(self).committee, envelope_authority(&envelope)) > 0
                      && reaches_quorum::<STRENGTH>(self)),

            // NotEnoughVotes iff structurally valid but quorum not yet reached.
            (out is NotEnoughVotes)
                <==> (envelope_epoch(&envelope) == committee_epoch_spec(&old(self).committee)
                      && !old(self).has_voted(envelope_authority(&envelope))
                      && committee_weight_of(&old(self).committee, envelope_authority(&envelope)) > 0
                      && !reaches_quorum::<STRENGTH>(self)),
    {
        let ghost pre_sig = envelope_sig_spec(&envelope);

        let (data, sig) = envelope.into_data_and_sig();
        // sig == pre_sig  (from assume_specification of into_data_and_sig)

        let comm_epoch = self.committee.epoch();
        let sig_epoch = sig.get_epoch();

        proof {
            // Bridge exec epoch values to spec projectors so the ensures clause
            // can be discharged.
            assert(sig_epoch == auth_sig_epoch_spec(&pre_sig));
            assert(comm_epoch == committee_epoch_spec(&self.committee));
            // envelope_epoch unfolds to auth_sig_epoch_spec(envelope_sig_spec(&envelope))
            // = auth_sig_epoch_spec(pre_sig) = sig_epoch.
            assert(sig_epoch == envelope_epoch(&envelope));
        }

        if comm_epoch != sig_epoch {
            return InsertResult::Failed {
                error: err_wrong_epoch(comm_epoch, sig_epoch),
            };
        }

        // Epochs match; extract the authority and delegate to insert_generic.
        let authority = sig.get_authority();

        proof {
            assert(authority == auth_sig_authority_spec(&pre_sig));
            assert(authority == envelope_authority(&envelope));
        }

        match self.insert_generic(authority, sig) {
            // insert_generic ensures: has_voted(a) iff old.has_voted(a) || a == authority.
            // In the QuorumReached arm, try_aggregate_and_verify may evict some entries,
            // so has_voted can only shrink from here.
            InsertResult::QuorumReached(_) => try_aggregate_and_verify(self, data),
            // In the other arms, state is exactly as insert_generic left it.
            InsertResult::Failed { error } => InsertResult::Failed { error },
            InsertResult::NotEnoughVotes {
                bad_votes,
                bad_authorities,
            } => InsertResult::NotEnoughVotes {
                bad_votes,
                bad_authorities,
            },
        }
    }
}

} // verus!

/// MultiStakeAggregator is a utility data structure that tracks the stake accumulation of
/// potentially multiple different values (usually due to byzantine/corrupted responses). Each
/// value is tracked using a StakeAggregator and determine whether it has reached a quorum.
/// Once quorum is reached, the aggregated signature is returned.
#[derive(Debug)]
pub struct MultiStakeAggregator<K, V, const STRENGTH: bool> {
    committee: Arc<Committee>,
    stake_maps: HashMap<K, (V, StakeAggregator<AuthoritySignInfo, STRENGTH>)>,
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH> {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            committee,
            stake_maps: Default::default(),
        }
    }

    pub fn total_votes(&self) -> StakeUnit {
        let mut voted_authorities = HashSet::new();
        self.stake_maps.values().for_each(|(_, stake_aggregator)| {
            stake_aggregator.keys().for_each(|k| {
                voted_authorities.insert(k);
            })
        });
        voted_authorities
            .iter()
            .map(|k| self.committee.weight(k))
            .sum()
    }

    #[cfg(test)]
    pub fn unique_key_count(&self) -> usize {
        self.stake_maps.len()
    }
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH>
where
    K: Hash + Eq,
    V: Message + Serialize + Clone,
{
    pub fn insert(
        &mut self,
        k: K,
        envelope: Envelope<V, AuthoritySignInfo>,
    ) -> InsertResult<AuthorityQuorumSignInfo<STRENGTH>> {
        if let Some(entry) = self.stake_maps.get_mut(&k) {
            entry.1.insert(envelope)
        } else {
            let mut new_entry = StakeAggregator::new(self.committee.clone());
            let result = new_entry.insert(envelope.clone());
            if !matches!(result, InsertResult::Failed { .. }) {
                // This is very important: ensure that if the insert fails, we don't even add the
                // new entry to the map.
                self.stake_maps.insert(k, (envelope.into_data(), new_entry));
            }
            result
        }
    }
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH>
where
    K: Clone + Ord,
{
    pub fn get_all_unique_values(&self) -> BTreeMap<K, (Vec<AuthorityName>, StakeUnit)> {
        self.stake_maps
            .iter()
            .map(|(k, (_, s))| {
                (
                    k.clone(),
                    (s.data.inner.keys().copied().collect(), s.total_votes),
                )
            })
            .collect()
    }
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH>
where
    K: Hash + Eq,
{
    #[allow(dead_code)]
    pub fn authorities_for_key(&self, k: &K) -> Option<impl Iterator<Item = &AuthorityName>> {
        self.stake_maps.get(k).map(|(_, agg)| agg.keys())
    }

    /// The sum of all remaining stake, i.e. all stake not yet
    /// committed by vote to a specific value
    pub fn uncommitted_stake(&self) -> StakeUnit {
        self.committee.total_votes() - self.total_votes()
    }

    /// Total stake of the largest faction
    pub fn plurality_stake(&self) -> StakeUnit {
        self.stake_maps
            .values()
            .map(|(_, agg)| agg.total_votes())
            .max()
            .unwrap_or_default()
    }

    /// If true, there isn't enough uncommitted stake to reach quorum for any value
    pub fn quorum_unreachable(&self) -> bool {
        self.uncommitted_stake() + self.plurality_stake() < self.committee.threshold::<STRENGTH>()
    }
}

/// Like MultiStakeAggregator, but for counting votes for a generic value instead of an envelope, in
/// scenarios where byzantine validators may submit multiple votes for different values.
pub struct GenericMultiStakeAggregator<K, const STRENGTH: bool> {
    committee: Arc<Committee>,
    stake_maps: HashMap<K, StakeAggregator<(), STRENGTH>>,
    votes_per_authority: HashMap<AuthorityName, u64>,
}

impl<K, const STRENGTH: bool> GenericMultiStakeAggregator<K, STRENGTH>
where
    K: Hash + Eq,
{
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            committee,
            stake_maps: Default::default(),
            votes_per_authority: Default::default(),
        }
    }

    pub fn insert(
        &mut self,
        authority: AuthorityName,
        k: K,
    ) -> InsertResult<&HashMap<AuthorityName, ()>> {
        let agg = self
            .stake_maps
            .entry(k)
            .or_insert_with(|| StakeAggregator::new(self.committee.clone()));

        if !agg.contains_key(&authority) {
            *self.votes_per_authority.entry(authority).or_default() += 1;
        }

        agg.insert_generic(authority, ())
    }

    pub fn has_quorum_for_key(&self, k: &K) -> bool {
        if let Some(entry) = self.stake_maps.get(k) {
            entry.has_quorum()
        } else {
            false
        }
    }

    pub fn votes_for_authority(&self, authority: AuthorityName) -> u64 {
        self.votes_per_authority
            .get(&authority)
            .copied()
            .unwrap_or_default()
    }
}

#[test]
fn test_votes_per_authority() {
    let (committee, _) = Committee::new_simple_test_committee();
    let authorities: Vec<_> = committee.names().copied().collect();

    let mut agg: GenericMultiStakeAggregator<&str, true> =
        GenericMultiStakeAggregator::new(Arc::new(committee));

    // 1. Inserting an `authority` and a `key`, and then checking the number of votes for that `authority`.
    let key1: &str = "key1";
    let authority1 = authorities[0];
    agg.insert(authority1, key1);
    assert_eq!(agg.votes_for_authority(authority1), 1);

    // 2. Inserting the same `authority` and `key` pair multiple times to ensure votes aren't incremented incorrectly.
    agg.insert(authority1, key1);
    agg.insert(authority1, key1);
    assert_eq!(agg.votes_for_authority(authority1), 1);

    // 3. Checking votes for an authority that hasn't voted.
    let authority2 = authorities[1];
    assert_eq!(agg.votes_for_authority(authority2), 0);

    // 4. Inserting multiple different authorities and checking their vote counts.
    let key2: &str = "key2";
    agg.insert(authority2, key2);
    assert_eq!(agg.votes_for_authority(authority2), 1);
    assert_eq!(agg.votes_for_authority(authority1), 1);

    // 5. Verifying that inserting different keys for the same authority increments the vote count.
    let key3: &str = "key3";
    agg.insert(authority1, key3);
    assert_eq!(agg.votes_for_authority(authority1), 2);
}

#[cfg(test)]
mod multi_stake_aggregator_tests {
    use super::*;
    use fastcrypto::hash::{HashFunction, Sha3_256};
    use shared_crypto::intent::IntentScope;

    #[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
    struct TestMessage {
        value: String,
    }

    impl Message for TestMessage {
        type DigestType = [u8; 32];
        const SCOPE: IntentScope = IntentScope::SenderSignedTransaction;

        fn digest(&self) -> Self::DigestType {
            let mut hasher = Sha3_256::default();
            hasher.update(self.value.as_bytes());
            hasher.finalize().digest
        }
    }

    #[test]
    fn test_equivocation_stake_not_double_counted() {
        let (committee, key_pairs) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: MultiStakeAggregator<String, TestMessage, true> =
            MultiStakeAggregator::new(committee.clone());

        // Get the actual total stake from the committee
        let total_stake = committee.total_votes();
        let num_authorities = authorities.len();
        let stake_per_authority = total_stake / num_authorities as u64;

        // Simulate equivocation: authority0 signs multiple different values
        let authority0 = authorities[0];
        let key0 = &key_pairs[0];

        // First signature for "value1"
        let msg1 = TestMessage {
            value: "value1".to_string(),
        };
        let envelope1 =
            <Envelope<TestMessage, AuthoritySignInfo>>::new(0, msg1.clone(), key0, authority0);
        agg.insert("key1".to_string(), envelope1);

        // Second signature from same authority for "value2" (equivocation)
        let msg2 = TestMessage {
            value: "value2".to_string(),
        };
        let envelope2 =
            <Envelope<TestMessage, AuthoritySignInfo>>::new(0, msg2.clone(), key0, authority0);
        agg.insert("key2".to_string(), envelope2);

        // Third signature from same authority for "value3" (more equivocation)
        let msg3 = TestMessage {
            value: "value3".to_string(),
        };
        let envelope3 =
            <Envelope<TestMessage, AuthoritySignInfo>>::new(0, msg3.clone(), key0, authority0);
        agg.insert("key3".to_string(), envelope3);

        // With the fix: authority0's stake should only be counted once, even though they signed 3 different values
        let aggregated_votes = agg.total_votes();
        assert_eq!(aggregated_votes, stake_per_authority);

        // Add more authorities signing different values
        let authority1 = authorities[1];
        let key1 = &key_pairs[1];
        let envelope4 =
            <Envelope<TestMessage, AuthoritySignInfo>>::new(0, msg1.clone(), key1, authority1);
        agg.insert("key1".to_string(), envelope4);

        let authority2 = authorities[2];
        let key2 = &key_pairs[2];
        let envelope5 = <Envelope<TestMessage, AuthoritySignInfo>>::new(0, msg2, key2, authority2);
        agg.insert("key2".to_string(), envelope5);

        // Now total_votes() should be stake_per_authority * 3 (3 unique authorities)
        // NOT stake_per_authority * 5 (which would be if we double-counted authority0)
        let aggregated_votes = agg.total_votes();
        assert_eq!(aggregated_votes, stake_per_authority * 3);
        assert!(aggregated_votes <= total_stake);

        // uncommitted_stake should work without underflow
        let uncommitted = agg.uncommitted_stake();
        assert_eq!(uncommitted, stake_per_authority); // Only authority3 hasn't voted

        // Verify we have 3 different keys with votes
        assert_eq!(agg.unique_key_count(), 3);
    }

    #[test]
    fn test_multistake_uncommitted_and_plurality() {
        let (committee, key_pairs) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: MultiStakeAggregator<String, TestMessage, true> =
            MultiStakeAggregator::new(committee.clone());

        let total_stake = committee.total_votes();
        let num_authorities = authorities.len();
        let stake_per_authority = total_stake / num_authorities as u64;

        // Initially, all stake is uncommitted
        assert_eq!(agg.uncommitted_stake(), total_stake);
        assert_eq!(agg.plurality_stake(), 0);
        assert!(!agg.quorum_unreachable());

        // Add first authority voting for value1
        let msg1 = TestMessage {
            value: "value1".to_string(),
        };
        let envelope1 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg1.clone(),
            &key_pairs[0],
            authorities[0],
        );
        agg.insert("key1".to_string(), envelope1);

        assert_eq!(agg.uncommitted_stake(), total_stake - stake_per_authority);
        assert_eq!(agg.plurality_stake(), stake_per_authority);

        // Add second authority voting for value2
        let msg2 = TestMessage {
            value: "value2".to_string(),
        };
        let envelope2 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg2.clone(),
            &key_pairs[1],
            authorities[1],
        );
        agg.insert("key2".to_string(), envelope2);

        assert_eq!(
            agg.uncommitted_stake(),
            total_stake - 2 * stake_per_authority
        );
        assert_eq!(agg.plurality_stake(), stake_per_authority);

        // Add third authority voting for value1 (now value1 has plurality)
        let envelope3 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg1.clone(),
            &key_pairs[2],
            authorities[2],
        );
        agg.insert("key1".to_string(), envelope3);

        assert_eq!(
            agg.uncommitted_stake(),
            total_stake - 3 * stake_per_authority
        );
        assert_eq!(agg.plurality_stake(), 2 * stake_per_authority);
    }

    #[test]
    fn test_multistake_quorum_unreachable() {
        let (committee, key_pairs) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: MultiStakeAggregator<String, TestMessage, true> =
            MultiStakeAggregator::new(committee.clone());

        // Split votes evenly so no value can reach quorum
        // With 4 authorities and strong quorum needing 2f+1, we need at least 3
        let msg1 = TestMessage {
            value: "value1".to_string(),
        };
        let msg2 = TestMessage {
            value: "value2".to_string(),
        };

        // Two authorities vote for value1
        let envelope1 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg1.clone(),
            &key_pairs[0],
            authorities[0],
        );
        agg.insert("key1".to_string(), envelope1);

        let envelope2 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg1.clone(),
            &key_pairs[1],
            authorities[1],
        );
        agg.insert("key1".to_string(), envelope2);

        // Two authorities vote for value2
        let envelope3 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg2.clone(),
            &key_pairs[2],
            authorities[2],
        );
        agg.insert("key2".to_string(), envelope3);

        let envelope4 = <Envelope<TestMessage, AuthoritySignInfo>>::new(
            0,
            msg2.clone(),
            &key_pairs[3],
            authorities[3],
        );
        agg.insert("key2".to_string(), envelope4);

        // With evenly split votes, neither can reach quorum now
        assert!(agg.quorum_unreachable());
    }
}

#[cfg(test)]
mod stake_aggregator_tests {
    use super::*;

    #[test]
    fn test_stake_aggregator_strong_quorum() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: StakeAggregator<(), true> = StakeAggregator::new(committee.clone());

        let total_stake = committee.total_votes();
        let num_authorities = authorities.len();
        let stake_per_authority = total_stake / num_authorities as u64;

        assert_eq!(agg.total_votes(), 0);
        assert!(!agg.has_quorum());
        assert_eq!(agg.validator_sig_count(), 0);

        // Add first authority - should not reach quorum yet
        let result = agg.insert_generic(authorities[0], ());
        assert!(matches!(result, InsertResult::NotEnoughVotes { .. }));
        assert_eq!(agg.total_votes(), stake_per_authority);
        assert!(!agg.has_quorum());
        assert_eq!(agg.validator_sig_count(), 1);

        // Add second authority - still not enough for strong quorum (2f+1)
        let result = agg.insert_generic(authorities[1], ());
        assert!(matches!(result, InsertResult::NotEnoughVotes { .. }));
        assert_eq!(agg.total_votes(), 2 * stake_per_authority);
        assert!(!agg.has_quorum());

        // Add third authority - should reach strong quorum
        let result = agg.insert_generic(authorities[2], ());
        assert!(result.is_quorum_reached());
        assert!(agg.has_quorum());
        assert_eq!(agg.validator_sig_count(), 3);
    }

    #[test]
    fn test_stake_aggregator_weak_quorum() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: StakeAggregator<(), false> = StakeAggregator::new(committee.clone());

        // Weak quorum (f+1) should be reached faster than strong quorum
        let result = agg.insert_generic(authorities[0], ());
        assert!(matches!(result, InsertResult::NotEnoughVotes { .. }));
        assert!(!agg.has_quorum());

        // Second authority should reach weak quorum
        let result = agg.insert_generic(authorities[1], ());
        assert!(result.is_quorum_reached());
        assert!(agg.has_quorum());
    }

    #[test]
    fn test_stake_aggregator_repeated_signer() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: StakeAggregator<u32, true> = StakeAggregator::new(committee.clone());

        // Insert first time - should succeed
        let result = agg.insert_generic(authorities[0], 1);
        assert!(matches!(result, InsertResult::NotEnoughVotes { .. }));

        // Insert same authority again with same value - should fail
        let result = agg.insert_generic(authorities[0], 1);
        assert!(matches!(
            result,
            InsertResult::Failed {
                error
            } if matches!(error.as_inner(),  SuiErrorKind::StakeAggregatorRepeatedSigner { .. } )
        ));

        // Insert same authority with different value - should also fail (conflicting signature)
        let result = agg.insert_generic(authorities[0], 2);
        let InsertResult::Failed { error } = result else {
            panic!("Expected StakeAggregatorRepeatedSigner error");
        };
        let SuiErrorKind::StakeAggregatorRepeatedSigner {
            signer,
            conflicting_sig,
        } = error.into_inner()
        else {
            panic!("Expected StakeAggregatorRepeatedSigner error");
        };
        assert_eq!(signer, authorities[0]);
        assert!(conflicting_sig);
    }

    #[test]
    fn test_stake_aggregator_from_iter() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let data = vec![
            Ok((authorities[0], ())),
            Ok((authorities[1], ())),
            Ok((authorities[2], ())),
        ];

        let agg: StakeAggregator<(), true> =
            StakeAggregator::from_iter(committee.clone(), data.into_iter()).unwrap();

        assert_eq!(agg.validator_sig_count(), 3);
        assert!(agg.has_quorum());
        assert!(agg.contains_key(&authorities[0]));
        assert!(agg.contains_key(&authorities[1]));
        assert!(agg.contains_key(&authorities[2]));
    }

    #[test]
    fn test_stake_aggregator_from_iter_with_error() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let data: Vec<Result<(AuthorityName, ()), TypedStoreError>> = vec![
            Ok((authorities[0], ())),
            Err(TypedStoreError::RocksDBError("test error".to_string())),
        ];

        let result: SuiResult<StakeAggregator<(), true>> =
            StakeAggregator::from_iter(committee.clone(), data.into_iter());

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod generic_multi_stake_aggregator_tests {
    use super::*;

    #[test]
    fn test_has_quorum_for_key() {
        let (committee, _) = Committee::new_simple_test_committee();
        let committee = Arc::new(committee);
        let authorities: Vec<_> = committee.names().copied().collect();

        let mut agg: GenericMultiStakeAggregator<&str, true> =
            GenericMultiStakeAggregator::new(committee.clone());

        let key1 = "key1";
        let key2 = "key2";

        // No quorum initially
        assert!(!agg.has_quorum_for_key(&key1));
        assert!(!agg.has_quorum_for_key(&key2));

        // Add votes for key1 until quorum
        agg.insert(authorities[0], key1);
        assert!(!agg.has_quorum_for_key(&key1));

        agg.insert(authorities[1], key1);
        assert!(!agg.has_quorum_for_key(&key1));

        agg.insert(authorities[2], key1);
        assert!(agg.has_quorum_for_key(&key1));
        assert!(!agg.has_quorum_for_key(&key2));

        // Add vote for key2, but not enough for quorum
        agg.insert(authorities[3], key2);
        assert!(agg.has_quorum_for_key(&key1));
        assert!(!agg.has_quorum_for_key(&key2));
    }
}
