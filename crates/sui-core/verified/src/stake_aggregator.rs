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
use tracing::warn;
use typed_store::TypedStoreError;
use vstd::prelude::*;

verus! {

#[cfg(verus_only)]
use crate::verus_shims::{
    committee_threshold_spec, committee_unique, committee_weight_of,
    lemma_voted_weight_empty, lemma_voted_weight_insert, voted_weight,
};

// ---------------------------------------------------------------------------
// AuthorityMap
// ---------------------------------------------------------------------------
//
// Thin wrapper around `HashMap<AuthorityName, S>` that adds the iteration
// API (`values`, `keys`, etc.) missing from `HashMapWithView`, while still
// giving verified code a clean `Map<AuthorityName, S>` view.
//
// Why not just use `HashMapWithView<AuthorityName, S>` directly?
// vstd's `HashMapWithView` has no `iter()` / `values()` / `IntoIterator`
// — the unverified `AuthoritySignInfo` specialisation below needs those.
// `AuthorityMap` bridges the gap: verified code uses the specced methods
// below; unverified code reaches `self.data.inner` for iteration.
//
// The view is `open` and delegates to the raw HashMap's own view (which
// vstd provides in `vstd::view`). With `obeys_key_model::<AuthorityName>()`
// axiomatised in `verus_shims`, no further axioms are needed for the
// view to be consistent.

#[derive(Debug)]
pub struct AuthorityMap<S> {
    pub inner: HashMap<AuthorityName, S>,
}

impl<S> View for AuthorityMap<S> {
    type V = Map<AuthorityName, S>;
    // Transparent: self@ unfolds to self.inner@ (the raw HashMap's view).
    // No uninterpreted axiom needed — the HashMap view is grounded in vstd.
    open spec fn view(&self) -> Map<AuthorityName, S> { self.inner@ }
}

impl<S> AuthorityMap<S> {
    #[verifier::external_body]
    pub fn new() -> (out: Self)
        ensures
            // Fresh map is empty and finite.
            out@ =~= Map::<AuthorityName, S>::empty(),
            out@.dom().finite(),
    {
        Self { inner: HashMap::new() }
    }

    #[verifier::external_body]
    pub fn contains_key(&self, name: &AuthorityName) -> (b: bool)
        // Result mirrors the ghost map.
        ensures b == self@.contains_key(*name),
    {
        self.inner.contains_key(name)
    }

    #[verifier::external_body]
    pub fn insert_new(&mut self, name: AuthorityName, value: S)
        requires
            // Caller promises the name is not already present.
            !old(self)@.contains_key(name),
            old(self)@.dom().finite(),
        ensures
            // After insert: view extends by (name, value); finiteness preserved.
            self@ =~= old(self)@.insert(name, value),
            self@.dom().finite(),
    {
        self.inner.insert(name, value);
    }

    /// Production uses this only for the `conflicting_sig` metadata in
    /// errors, which does not affect the sum invariant. No useful spec.
    #[verifier::external_body]
    pub fn get_value<'a>(&'a self, name: &AuthorityName) -> (out: Option<&'a S>) {
        self.inner.get(name)
    }
}

impl<S> Default for AuthorityMap<S> {
    #[verifier::external_body]
    fn default() -> (out: Self)
        // Default == new(): empty map.
        ensures
            out@ =~= Map::<AuthorityName, S>::empty(),
            out@.dom().finite(),
    {
        Self::new()
    }
}

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

} // verus!

verus! {

/// StakeAggregator allows us to keep track of the total stake of a set of validators.
/// STRENGTH indicates whether we want a strong quorum (2f+1) or a weak quorum (f+1).
#[derive(Debug)]
pub struct StakeAggregator<S, const STRENGTH: bool> {
    pub data: AuthorityMap<S>,
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
            data: AuthorityMap::default(),
            total_votes: 0,
            committee,
        }
    }

    /// A generic version of inserting arbitrary type of V (e.g. void type).
    /// If V is AuthoritySignInfo, the `insert` function should be used instead since it does extra
    /// checks and aggregations in the end.
    /// Returns Map authority -> S, without aggregating it.
    /// If you want to get an aggregated signature instead, use `StakeAggregator::insert`
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
            // Variant-specific guarantees describing what each return tells the caller.
            match out {
                // Quorum reached: the authority was new, had positive weight, and
                // the total now meets or exceeds the threshold.
                InsertResult::QuorumReached(_) =>
                    !old(self).data@.contains_key(authority)
                    && committee_weight_of(&self.committee, authority) > 0
                    && self.total_votes
                        >= committee_threshold_spec(&self.committee, STRENGTH),
                // Counted but below threshold: same as above but total < threshold,
                // and the bookkeeping fields are zero/empty (no bad signatures yet).
                InsertResult::NotEnoughVotes { bad_votes, bad_authorities } =>
                    !old(self).data@.contains_key(authority)
                    && committee_weight_of(&self.committee, authority) > 0
                    && self.total_votes
                        < committee_threshold_spec(&self.committee, STRENGTH)
                    && bad_votes == 0
                    && bad_authorities@.len() == 0,
                // Failed: either the signer was already present (state fully
                // unchanged) or the authority has zero weight (domain extended,
                // total unchanged). Total is always preserved.
                InsertResult::Failed { .. } =>
                    self.total_votes == old(self).total_votes
                    && (
                        (old(self).data@.contains_key(authority)
                            && self.data@ =~= old(self).data@)
                        || (committee_weight_of(&self.committee, authority) == 0
                            && self.data@.dom()
                                =~= old(self).data@.dom().insert(authority))
                    ),
            },
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

impl<const STRENGTH: bool> StakeAggregator<AuthoritySignInfo, STRENGTH> {
    /// Insert an authority signature. This is the primary way to use the aggregator and a few
    /// dedicated checks are performed to make sure things work.
    /// If quorum is reached, we return AuthorityQuorumSignInfo directly.
    pub fn insert<T: Message + Serialize>(
        &mut self,
        envelope: Envelope<T, AuthoritySignInfo>,
    ) -> InsertResult<AuthorityQuorumSignInfo<STRENGTH>> {
        let (data, sig) = envelope.into_data_and_sig();
        if self.committee.epoch != sig.epoch {
            return InsertResult::Failed {
                error: SuiErrorKind::WrongEpoch {
                    expected_epoch: self.committee.epoch,
                    actual_epoch: sig.epoch,
                }
                .into(),
            };
        }
        match self.insert_generic(sig.authority, sig) {
            InsertResult::QuorumReached(_) => {
                match AuthorityQuorumSignInfo::<STRENGTH>::new_from_auth_sign_infos(
                    self.data.inner.values().cloned().collect(),
                    self.committee(),
                ) {
                    Ok(aggregated) => {
                        match aggregated.verify_secure(
                            &data,
                            Intent::sui_app(T::SCOPE),
                            self.committee(),
                        ) {
                            // In the happy path, the aggregated signature verifies ok and no need to verify
                            // individual.
                            Ok(_) => InsertResult::QuorumReached(aggregated),
                            Err(_) => {
                                // If the aggregated signature fails to verify, fallback to iterating through
                                // all signatures and verify individually. Decrement total votes and continue
                                // to find new authority for signature to reach the quorum.
                                //
                                // TODO(joyqvq): It is possible for the aggregated signature to fail every time
                                // when the latest one single signature fails to verify repeatedly, and trigger
                                // this for loop to run. This can be optimized by caching single sig verification
                                // result only verify the net new ones.
                                let mut bad_votes = 0;
                                let mut bad_authorities = vec![];
                                for (name, sig) in &self.data.inner.clone() {
                                    if let Err(err) = sig.verify_secure(
                                        &data,
                                        Intent::sui_app(T::SCOPE),
                                        self.committee(),
                                    ) {
                                        // TODO(joyqvq): Currently, the aggregator cannot do much with an authority that
                                        // always returns an invalid signature other than saving to errors in state. It
                                        // is possible to add the authority to a denylist or  punish the byzantine authority.
                                        warn!(name=?name.concise(), "Bad stake from validator: {:?}", err);
                                        self.data.inner.remove(name);
                                        let votes = self.committee.weight(name);
                                        self.total_votes -= votes;
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
            // The following is necessary to change the template type of InsertResult.
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

verus! {

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
