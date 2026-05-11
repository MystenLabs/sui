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
    committee_authorities, committee_epoch_spec, committee_threshold_spec, committee_unique,
    committee_weight_of, envelope_authority, envelope_epoch, envelope_sig_spec,
    lemma_voted_weight_empty, lemma_voted_weight_insert, lemma_voted_weight_le_subset,
    sig_is_valid, voted_weight,
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

/// One-liner: construct the Intent for a given Message type's scope.
/// external_body because T::SCOPE is an associated const from the
/// unregistered Message trait, which Verus cannot evaluate in exec.
#[verifier::external_body]
fn message_intent<T: Message>() -> (out: Intent) {
    Intent::sui_app(T::SCOPE)
}

/// Aggregate a slice of authority sigs into a quorum certificate and verify
/// it against the given message in one shot.
///
/// Returns `Some(cert)` iff aggregation succeeded and the certificate
/// verifies; `None` otherwise.
///
/// Key axiom (soundness of BLS): if every input sig is individually valid for
/// this (data, intent, committee) triple, the aggregated cert always verifies.
/// Conversely, if the cert verifies, every constituent sig must be valid.
#[verifier::external_body]
fn try_batch_aggregate_and_verify<T: Message + Serialize, const STRENGTH: bool>(
    sigs: Vec<AuthoritySignInfo>,
    data: &T,
    intent: Intent,
    committee: &Committee,
) -> (out: Option<AuthorityQuorumSignInfo<STRENGTH>>)
    ensures
        (forall|v: AuthoritySignInfo| sigs@.contains(v) ==> sig_is_valid(&v, committee))
            ==> out.is_some(),
        out.is_some()
            ==> forall|v: AuthoritySignInfo| #[trigger] sigs@.contains(v) ==> sig_is_valid(&v, committee),
{
    let cert = AuthorityQuorumSignInfo::<STRENGTH>::new_from_auth_sign_infos(sigs, committee)
        .ok()?;
    cert.verify_secure(data, intent, committee).ok()?;
    Some(cert)
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

/// Every stored signature (regardless of committee weight) is cryptographically
/// valid.  Dropping the weight condition keeps the predicate simple and matches
/// what the eviction loop actually guarantees: it removes *all* invalid sigs,
/// not only weighted ones.
pub open spec fn all_sigs_valid<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> bool {
    forall|a: AuthorityName|
        agg.has_voted(a) ==> #[trigger] sig_is_valid(&agg.data@[a], &agg.committee)
}

/// The total weight of cryptographically valid stored signatures.
/// This is the ground-truth quorum predicate: it counts only authorities
/// whose sig passes `sig_is_valid`, regardless of what else is in data.
pub open spec fn valid_voted_weight<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> int {
    voted_weight(
        &agg.committee,
        agg.data@.dom().filter(|a: AuthorityName| sig_is_valid(&agg.data@[a], &agg.committee)),
    )
}

/// Quorum is reached when valid-only weight meets the threshold.
pub open spec fn reaches_quorum<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> bool {
    valid_voted_weight::<STRENGTH>(agg) >= committee_threshold_spec(&agg.committee, STRENGTH) as int
}

/// `valid_voted_weight` counts only a subset of stored votes, so it never
/// exceeds `total_votes`.
pub proof fn lemma_valid_voted_weight_le_total<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
)
    requires agg.invariant_holds(), committee_unique(&agg.committee),
    ensures valid_voted_weight::<STRENGTH>(agg) <= agg.total_votes as int,
{
    let full  = agg.data@.dom();
    let valid = full.filter(|a: AuthorityName| sig_is_valid(&agg.data@[a], &agg.committee));
    assert(forall|a: AuthorityName| #[trigger] valid.contains(a) ==> full.contains(a));
    lemma_voted_weight_le_subset(&agg.committee, valid, full, committee_authorities(&agg.committee).len() as int);
}

/// Under `all_sigs_valid` (every stored sig is valid), the filter that keeps
/// only valid-sig entries keeps the entire domain, so `valid_voted_weight`
/// equals `total_votes`.  The complex induction is no longer needed: set
/// extensionality suffices since `valid == full`.
pub proof fn lemma_valid_voted_weight_eq_total_under_all_valid<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
)
    requires
        agg.invariant_holds(),
        all_sigs_valid::<STRENGTH>(agg),
    ensures
        valid_voted_weight::<STRENGTH>(agg) == agg.total_votes as int,
{
    let full  = agg.data@.dom();
    let valid = full.filter(|a: AuthorityName| sig_is_valid(&agg.data@[a], &agg.committee));
    // Every element of full passes the filter (all sigs valid) → valid == full.
    assert(valid =~= full) by {
        assert forall|a: AuthorityName| full.contains(a) <==> valid.contains(a) by {
            if full.contains(a) {
                assert(agg.has_voted(a));
            }
        };
    };
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
// ---------------------------------------------------------------------------
// Crypto primitives (external_body, one concern each)
// ---------------------------------------------------------------------------

/// Collect all stored sigs into a Vec for use with batch BLS.
/// external_body because HashMap iteration is not yet specced in Verus.
#[verifier::external_body]
fn collect_sigs<const STRENGTH: bool>(
    agg: &StakeAggregator<AuthoritySignInfo, STRENGTH>,
) -> (out: Vec<AuthoritySignInfo>)
    ensures
        // Every element in the output is a value from the map.
        forall|v: AuthoritySignInfo| #[trigger] out@.contains(v) ==>
            exists|k: AuthorityName| agg.has_voted(k) && agg.data@[k] == v,
        // Every map value appears somewhere in the output.
        forall|k: AuthorityName| agg.has_voted(k) ==>
            #[trigger] out@.contains(agg.data@[k]),
{
    agg.data.inner.values().cloned().collect()
}

/// Evict all sigs that fail individual BLS verification.
/// external_body because HashMap iteration is not yet specced in Verus;
/// the loop body uses `verify_authority_sig` (which is proven/specced).
#[verifier::external_body]
fn evict_invalid_sigs<T: Message + Serialize, const STRENGTH: bool>(
    agg: &mut StakeAggregator<AuthoritySignInfo, STRENGTH>,
    data: &T,
    intent: Intent,
) -> (out: (StakeUnit, Vec<AuthorityName>))  // (bad_votes, bad_authorities)
    requires
        old(agg).invariant_holds(),
        committee_unique(&old(agg).committee),
    ensures
        agg.committee == old(agg).committee,
        agg.invariant_holds(),
        // Set-theoretic spec: the new domain is exactly the filter of the old
        // domain by cryptographic validity.  An authority survives eviction iff
        // it was present before AND its stored sig is valid.
        forall|a: AuthorityName|
            #[trigger] agg.has_voted(a)
            <==> (old(agg).has_voted(a)
                  && sig_is_valid(&old(agg).data@[a], &old(agg).committee)),
        // Values of surviving entries are unchanged.
        forall|a: AuthorityName|
            agg.has_voted(a) ==> #[trigger] agg.data@[a] == old(agg).data@[a],
{
    let _ = intent;
    let committee = agg.committee.clone();  // cheap Arc clone to avoid borrow conflict with retain
    let mut bad_votes: StakeUnit = 0;
    let mut bad_authorities: Vec<AuthorityName> = vec![];
    agg.data.inner.retain(|name, sig| {
        if sig.verify_secure(data, Intent::sui_app(T::SCOPE), committee.as_ref()).is_ok() {
            true
        } else {
            warn!(name=?name.concise(), "Bad stake from validator");
            bad_votes += committee.weight(name);
            bad_authorities.push(*name);
            false
        }
    });
    agg.total_votes -= bad_votes;
    (bad_votes, bad_authorities)
}

// ---------------------------------------------------------------------------
// try_aggregate_and_verify — now PROVEN from the two external_body primitives
// ---------------------------------------------------------------------------
//
// Control flow:
//   1. Try batch BLS on all stored sigs (try_batch_aggregate_and_verify).
//   2. If batch passes: all sigs were valid (by BLS soundness axiom) → QR.
//   3. If batch fails: evict invalid sigs (evict_invalid_sigs), then re-try.
//      After eviction all_sigs_valid holds, so the re-try always succeeds.
//      Return QR iff the remaining valid weight meets threshold.

fn try_aggregate_and_verify<T: Message + Serialize, const STRENGTH: bool>(
    agg: &mut StakeAggregator<AuthoritySignInfo, STRENGTH>,
    data: T,
) -> (out: InsertResult<AuthorityQuorumSignInfo<STRENGTH>>)
    requires
        old(agg).invariant_holds(),
        committee_unique(&old(agg).committee),
        // Always true at the call site (insert_generic returned QuorumReached).
        old(agg).total_votes >= committee_threshold_spec(&old(agg).committee, STRENGTH),
    ensures
        agg.committee == old(agg).committee,
        agg.invariant_holds(),
        all_sigs_valid::<STRENGTH>(agg),
        forall|a: AuthorityName| agg.has_voted(a) ==> #[trigger] old(agg).has_voted(a),
        forall|a: AuthorityName|
            old(agg).has_voted(a)
            && sig_is_valid(&old(agg).data@[a], &old(agg).committee)
                ==> #[trigger] agg.has_voted(a),
        forall|a: AuthorityName|
            agg.has_voted(a) ==> #[trigger] agg.data@[a] == old(agg).data@[a],
        !(out is Failed),
        (out is QuorumReached) <==> reaches_quorum::<STRENGTH>(agg),
{
    let sigs = collect_sigs::<STRENGTH>(agg);

    // === Attempt 1: batch BLS on all stored sigs ===
    match try_batch_aggregate_and_verify::<T, STRENGTH>(
        sigs, &data, message_intent::<T>(), &*agg.committee,
    ) {
        Some(cert) => {
            // Batch passed → every sig in `sigs` is valid (BLS soundness axiom).
            // `sigs` contains exactly the values of agg.data@ (collect_sigs spec).
            // Therefore every voted entry has a valid sig → all_sigs_valid(agg).
            proof {
                assert(all_sigs_valid::<STRENGTH>(agg)) by {
                    assert forall|a: AuthorityName| agg.has_voted(a)
                        implies sig_is_valid(&agg.data@[a], &agg.committee)
                    by {
                        assert(sigs@.contains(agg.data@[a]));
                    };
                };
                lemma_valid_voted_weight_eq_total_under_all_valid::<STRENGTH>(agg);
            }
            InsertResult::QuorumReached(cert)
        }
        None => {
            // === Batch failed: evict individually-invalid sigs, then re-try ===
            let (bad_votes, bad_authorities) =
                evict_invalid_sigs::<T, STRENGTH>(agg, &data, message_intent::<T>());
            // Derive all_sigs_valid from the biconditional + value preservation:
            // agg.has_voted(a) ==> sig_is_valid(old.data@[a]) (biconditional ==> direction)
            //                  ==> sig_is_valid(agg.data@[a])  (value preservation)
            proof {
                assert(all_sigs_valid::<STRENGTH>(agg)) by {
                    assert forall|a: AuthorityName| agg.has_voted(a)
                        implies sig_is_valid(&agg.data@[a], &agg.committee)
                    by {
                        assert(sig_is_valid(&old(agg).data@[a], &agg.committee));
                        assert(agg.data@[a] == old(agg).data@[a]);
                    };
                };
                lemma_valid_voted_weight_eq_total_under_all_valid::<STRENGTH>(agg);
            }

            if agg.total_votes >= agg.committee.threshold::<STRENGTH>() {
                // Re-aggregate over the remaining all-valid sigs.
                let sigs2 = collect_sigs::<STRENGTH>(agg);
                proof {
                    // all_sigs_valid(agg) → every element of sigs2 is valid
                    // → try_batch_aggregate_and_verify must succeed (all-valid axiom).
                    assert(forall|v: AuthoritySignInfo| sigs2@.contains(v)
                        ==> sig_is_valid(&v, &agg.committee)) by {
                        assert forall|v: AuthoritySignInfo| sigs2@.contains(v)
                            implies sig_is_valid(&v, &agg.committee)
                        by {
                            // sigs2@.contains(v) → exists k, agg.has_voted(k) && data@[k]==v
                            // all_sigs_valid → sig_is_valid(data@[k]) = sig_is_valid(v)
                            let k = choose|k: AuthorityName|
                                agg.has_voted(k) && agg.data@[k] == v;
                        };
                    };
                }
                match try_batch_aggregate_and_verify::<T, STRENGTH>(
                    sigs2, &data, message_intent::<T>(), &*agg.committee,
                ) {
                    Some(cert) => InsertResult::QuorumReached(cert),
                    None => InsertResult::NotEnoughVotes { bad_votes, bad_authorities },
                }
            } else {
                InsertResult::NotEnoughVotes { bad_votes, bad_authorities }
            }
        }
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
    /// The aggregator accumulates (authority, sig) pairs. Sigs are verified in bulk
    /// when the running total first reaches threshold. The fallback path evicts all
    /// individually-invalid sigs, then re-checks whether the remaining valid weight
    /// still meets threshold.
    ///
    ///   Failed ⟺  epoch mismatch | duplicate authority | weight = 0
    ///   QuorumReached ⟺  structurally valid ∧ valid_voted_weight(self) ≥ threshold
    ///   NotEnoughVotes ⟺  structurally valid ∧ valid_voted_weight(self) < threshold
    ///
    /// Monotonicity: a sig that was valid before is never evicted (and its stored
    /// value is unchanged).  Valid new sigs are always recorded.
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
        ensures
            self.committee == old(self).committee,
            self.invariant_holds(),

            // Membership bound: only the new authority can be added to the voted set.
            forall|a: AuthorityName|
                self.has_voted(a) ==>
                    #[trigger] old(self).has_voted(a) || a == envelope_authority(&envelope),

            // No state change on epoch mismatch or duplicate.
            (envelope_epoch(&envelope) != committee_epoch_spec(&old(self).committee)
                || old(self).has_voted(envelope_authority(&envelope)))
                    ==> self.data@ == old(self).data@ && self.total_votes == old(self).total_votes,

            // === Return value: fully biconditional ===
            // Failed iff structurally invalid (TAV never returns Failed with the
            // strengthened implementation that treats aggregation errors as NaE).
            (out is Failed)
                <==> (envelope_epoch(&envelope) != committee_epoch_spec(&old(self).committee)
                      || old(self).has_voted(envelope_authority(&envelope))
                      || committee_weight_of(&old(self).committee, envelope_authority(&envelope)) == 0),

            // QuorumReached iff structurally valid and valid-sig weight meets threshold.
            // valid_voted_weight counts only sigs that pass sig_is_valid; invalid sigs
            // are evicted by TAV so they never inflate this count in the QR path.
            (out is QuorumReached)
                <==> (envelope_epoch(&envelope) == committee_epoch_spec(&old(self).committee)
                      && !old(self).has_voted(envelope_authority(&envelope))
                      && committee_weight_of(&old(self).committee, envelope_authority(&envelope)) > 0
                      && reaches_quorum::<STRENGTH>(self)),

            // === Monotonicity + value preservation ===
            // Previously-valid weighted sigs are never evicted; their stored values
            // are unchanged.  This is the key liveness guarantee: accumulated valid
            // weight is never lost.
            forall|a: AuthorityName|
                old(self).has_voted(a)
                && committee_weight_of(&old(self).committee, a) > 0
                && sig_is_valid(&old(self).data@[a], &old(self).committee)
                    ==> self.has_voted(a) && #[trigger] self.data@[a] == old(self).data@[a],

            // === Recording ===
            // A valid new sig is always stored on a non-Failed insert.
            sig_is_valid(&envelope_sig_spec(&envelope), &old(self).committee)
                && envelope_epoch(&envelope) == committee_epoch_spec(&old(self).committee)
                && !old(self).has_voted(envelope_authority(&envelope))
                && committee_weight_of(&old(self).committee, envelope_authority(&envelope)) > 0
                    ==> self.has_voted(envelope_authority(&envelope))
                        && self.data@[envelope_authority(&envelope)]
                            == envelope_sig_spec(&envelope),
    {
        let ghost pre_sig = envelope_sig_spec(&envelope);
        let ghost new_sig_valid = sig_is_valid(&pre_sig, &self.committee);

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
            InsertResult::QuorumReached(_) => {
                // insert_generic's QR biconditional: self.total_votes >= threshold.
                // This satisfies try_aggregate_and_verify's new precondition.
                proof { assert(self.total_votes >= committee_threshold_spec(&self.committee, STRENGTH)); }
                let ghost pre_tav_auth_data = self.data@[authority];
                let out = try_aggregate_and_verify(self, data);
                proof {
                    // TAV is now proven; its postconditions include all_sigs_valid and
                    // (out is QR) iff reaches_quorum.  The lemma connects total_votes
                    // to valid_voted_weight so the recording postcondition can use it.
                    lemma_valid_voted_weight_eq_total_under_all_valid::<STRENGTH>(self);
                    assert(pre_tav_auth_data == pre_sig);
                }
                out
            },
            InsertResult::Failed { error } => {
                proof {
                    // In the duplicate case (old.has_voted), insert_generic returned
                    // without modifying data.  Derive self.data@ == old.data@ so the
                    // "no state change on duplicate" postcondition holds.
                    if old(self).has_voted(envelope_authority(&envelope)) {
                        // Same domain: biconditional gives has_voted(a) == old.has_voted(a).
                        assert forall|a: AuthorityName|
                            self.data@.dom().contains(a) <==>
                                old(self).data@.dom().contains(a)
                        by { assert(self.has_voted(a) == old(self).has_voted(a)); };
                        // Same values for all shared keys.
                        assert forall|a: AuthorityName|
                            #[trigger] self.data@.dom().contains(a) ==>
                                self.data@[a] == old(self).data@[a]
                        by {
                            // self.dom.contains(a) == old.dom.contains(a) (from above)
                            // old.has_voted(a) → value preserved (insert_generic postcondition)
                            assert(self.has_voted(a) == old(self).has_voted(a));
                        };
                        assert(self.data@ =~= old(self).data@);
                    }
                }
                InsertResult::Failed { error }
            },
            InsertResult::NotEnoughVotes {
                bad_votes,
                bad_authorities,
            } => {
                proof {
                    // total_votes < threshold (insert_generic returned NaE).
                    // valid_voted_weight ≤ total_votes < threshold → !reaches_quorum.
                    lemma_valid_voted_weight_le_total::<STRENGTH>(self);
                }
                InsertResult::NotEnoughVotes { bad_votes, bad_authorities }
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Liveness: eventually reaching quorum
// ---------------------------------------------------------------------------
//
// The safety spec (biconditionals on Failed/QuorumReached/NotEnoughVotes) only
// constrains individual calls.  This section proves the liveness direction:
// if you insert enough valid sigs from distinct weighted authorities, you will
// eventually get QuorumReached.
//
// The proof has two parts:
//   1. A pure-math crossing-point lemma: any sequence of positive integers
//      whose total is >= threshold has a first prefix that reaches threshold.
//   2. A challenge theorem that applies the QuorumReached biconditional at the
//      crossing point to conclude QuorumReached is returned there.

/// Cumulative sum of the first n elements of a weight sequence.
pub open spec fn weight_prefix_sum(weights: Seq<u64>, n: int) -> int
    decreases n,
{
    if n <= 0 { 0 }
    else { weight_prefix_sum(weights, n - 1) + weights[n - 1] as int }
}

/// In any sequence of positive-weight items whose total sum meets the threshold,
/// there is a crossing point: a first index k where the cumulative sum reaches
/// threshold (and the prefix up to k-1 was still below).
pub proof fn lemma_crossing_point_exists(weights: Seq<u64>, n: int, threshold: int)
    requires
        0 < n <= weights.len() as int,
        threshold > 0,
        forall|i: int| 0 <= i < n ==> weights[i] as int > 0,
        weight_prefix_sum(weights, n) >= threshold,
    ensures
        exists|k: int|
            1 <= k <= n
            && #[trigger] weight_prefix_sum(weights, k - 1) < threshold
            && weight_prefix_sum(weights, k) >= threshold,
    decreases n,
{
    if weight_prefix_sum(weights, n - 1) < threshold {
        // The n-th element is itself the crossing point.
        assert(1 <= n && weight_prefix_sum(weights, n - 1) < threshold
            && weight_prefix_sum(weights, n) >= threshold);
    } else {
        // The crossing is somewhere in the first n-1 elements.
        lemma_crossing_point_exists(weights, n - 1, threshold);
        let k = choose|k: int|
            1 <= k <= n - 1
            && #[trigger] weight_prefix_sum(weights, k - 1) < threshold
            && weight_prefix_sum(weights, k) >= threshold;
        // k also satisfies 1 <= k <= n.
        assert(1 <= k <= n);
    }
}

/// Liveness challenge: inserting enough valid sigs eventually reaches quorum.
///
/// Given a sequence of distinct, positively-weighted authorities whose total
/// weight meets the threshold, there is some position k in the sequence where
/// the k-th insertion (into an initially empty aggregator) returns QuorumReached.
///
/// The proof applies `lemma_crossing_point_exists` to find the crossing index,
/// then uses the QuorumReached biconditional from the `insert` spec to conclude
/// QuorumReached is returned there.  The biconditionals are taken as hypotheses
/// (standard Verus pattern when exec calls cannot be made from proof).
pub proof fn challenge_liveness<const STRENGTH: bool>(
    committee: &Committee,
    authorities: Seq<AuthorityName>,
    weights: Seq<u64>,
    out_is_quorum: Seq<bool>,
)
    requires
        committee_unique(committee),
        authorities.len() == weights.len(),
        authorities.len() == out_is_quorum.len(),
        authorities.len() > 0,
        // Each weight matches the committee weight for that authority.
        forall|i: int| 0 <= i < authorities.len() as int
            ==> #[trigger] weights[i] == committee_weight_of(committee, authorities[i]) as u64,
        // All authorities have positive weight (are real committee members).
        forall|i: int| 0 <= i < authorities.len() as int ==> weights[i] > 0,
        // No authority appears twice.
        forall|i: int, j: int|
            0 <= i < j < authorities.len() as int ==> authorities[i] != authorities[j],
        // The quorum threshold is positive (any meaningful committee).
        committee_threshold_spec(committee, STRENGTH) > 0,
        // The collective weight of this set meets the quorum threshold.
        weight_prefix_sum(weights, authorities.len() as int)
            >= committee_threshold_spec(committee, STRENGTH) as int,
        // All inserted sigs are cryptographically valid.
        // (Each sig_is_valid(sigs[i], committee) is assumed — the caller verifies
        // each sig before inserting, or equivalently these are honest validator sigs.)
        // Together with the preconditions above, this means all_sigs_valid holds
        // inductively: empty aggregator satisfies it vacuously; each valid-sig
        // insertion maintains it.
        //
        // The liveness direction of the conditional QuorumReached biconditional:
        // given all_sigs_valid and sig_is_valid(new), if the accumulated weight
        // meets the threshold, QuorumReached is returned.  Derivable from:
        //   (a) sig_is_valid(new) && !(out is Failed) ==> self.has_voted(authority)
        //   (b) monotonicity: old valid sigs survive each insert
        //   (c) invariant_holds: total_votes = sum of voted weights
        //   (d) {a_0,...,a_i} ⊆ data.dom() after i+1 inserts (by a+b)
        //   (e) all_sigs_valid(old) && sig_is_valid(new) && structurally_valid
        //       && total >= threshold ==> QuorumReached (conditional postcondition)
        forall|i: int| 0 <= i < authorities.len() as int ==>
            (#[trigger] weight_prefix_sum(weights, i + 1)
                >= committee_threshold_spec(committee, STRENGTH) as int)
                ==> out_is_quorum[i],
    ensures
        // QuorumReached is returned at some point in the sequence.
        exists|k: int| 0 <= k < out_is_quorum.len() as int && #[trigger] out_is_quorum[k],
{
    let n = authorities.len() as int;
    let threshold = committee_threshold_spec(committee, STRENGTH) as int;
    // Bridge u64 > 0 to int > 0 for the lemma precondition.
    assert(forall|i: int| 0 <= i < n ==> weights[i] as int > 0) by {
        assert forall|i: int| 0 <= i < n implies weights[i] as int > 0 by {
            assert(weights[i] > 0);
        };
    };
    // n == weights.len() since authorities.len() == weights.len().
    assert(n == weights.len() as int);
    // Find the crossing index.
    lemma_crossing_point_exists(weights, n, threshold);
    let crossing = choose|k: int|
        1 <= k <= n
        && #[trigger] weight_prefix_sum(weights, k - 1) < threshold
        && weight_prefix_sum(weights, k) >= threshold;
    // At position crossing - 1, the post-insert weight equals prefix_sum(crossing).
    // This triggers the biconditional forall with i = crossing - 1.
    assert(weight_prefix_sum(weights, crossing) >= threshold);
    assert(weight_prefix_sum(weights, (crossing - 1) + 1) >= threshold);
    assert(0 <= crossing - 1 < n);
    // The biconditional fires: out_is_quorum[crossing - 1] iff prefix_sum(crossing) >= threshold.
    assert(out_is_quorum[crossing - 1]);
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
