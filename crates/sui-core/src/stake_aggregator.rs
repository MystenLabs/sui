// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use shared_crypto::intent::Intent;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait};
use sui_types::error::SuiError;
use sui_types::message_envelope::{Envelope, Message};
use tracing::warn;
use typed_store::TypedStoreError;

/// StakeAggregator allows us to keep track of the total stake of a set of validators.
/// STRENGTH indicates whether we want a strong quorum (2f+1) or a weak quorum (f+1).
#[derive(Debug)]
pub struct StakeAggregator<S, const STRENGTH: bool> {
    data: HashMap<AuthorityName, S>,
    total_votes: StakeUnit,
    committee: Arc<Committee>,
}

/// StakeAggregator is a utility data structure that allows us to aggregate a list of validator
/// signatures over time. A committee is used to determine whether we have reached sufficient
/// quorum (defined based on `STRENGTH`). The generic implementation does not require `S` to be
/// an actual signature, but just an indication that a specific validator has voted. A specialized
/// implementation for `AuthoritySignInfo` is followed below.
impl<S: Clone + Eq, const STRENGTH: bool> StakeAggregator<S, STRENGTH> {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            data: Default::default(),
            total_votes: Default::default(),
            committee,
        }
    }

    pub fn from_iter<I: Iterator<Item = Result<(AuthorityName, S), TypedStoreError>>>(
        committee: Arc<Committee>,
        data: I,
    ) -> Result<Self, TypedStoreError> {
        let mut this = Self::new(committee);
        for item in data {
            let (authority, s) = item?;
            this.insert_generic(authority, s);
        }
        Ok(this)
    }

    /// A generic version of inserting arbitrary type of V (e.g. void type).
    /// If V is AuthoritySignInfo, the `insert` function should be used instead since it does extra
    /// checks and aggregations in the end.
    /// Returns Map authority -> S, without aggregating it.
    /// If you want to get an aggregated signature instead, use `StakeAggregator::insert`
    pub fn insert_generic(
        &mut self,
        authority: AuthorityName,
        s: S,
    ) -> InsertResult<&HashMap<AuthorityName, S>> {
        match self.data.entry(authority) {
            Entry::Occupied(oc) => {
                return InsertResult::Failed {
                    error: SuiError::StakeAggregatorRepeatedSigner {
                        signer: authority,
                        conflicting_sig: oc.get() == &s,
                    },
                };
            }
            Entry::Vacant(va) => {
                va.insert(s);
            }
        }
        let votes = self.committee.weight(&authority);
        if votes > 0 {
            self.total_votes += votes;
            if self.total_votes >= self.committee.threshold::<STRENGTH>() {
                InsertResult::QuorumReached(&self.data)
            } else {
                InsertResult::NotEnoughVotes {
                    bad_votes: 0,
                    bad_authorities: vec![],
                }
            }
        } else {
            InsertResult::Failed {
                error: SuiError::InvalidAuthenticator,
            }
        }
    }

    pub fn contains_key(&self, authority: &AuthorityName) -> bool {
        self.data.contains_key(authority)
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    pub fn total_votes(&self) -> StakeUnit {
        self.total_votes
    }

    pub fn validator_sig_count(&self) -> usize {
        self.data.len()
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
                error: SuiError::WrongEpoch {
                    expected_epoch: self.committee.epoch,
                    actual_epoch: sig.epoch,
                },
            };
        }
        match self.insert_generic(sig.authority, sig) {
            InsertResult::QuorumReached(_) => {
                match AuthorityQuorumSignInfo::<STRENGTH>::new_from_auth_sign_infos(
                    self.data.values().cloned().collect(),
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
                                for (name, sig) in &self.data.clone() {
                                    if let Err(err) = sig.verify_secure(
                                        &data,
                                        Intent::sui_app(T::SCOPE),
                                        self.committee(),
                                    ) {
                                        // TODO(joyqvq): Currently, the aggregator cannot do much with an authority that
                                        // always returns an invalid signature other than saving to errors in state. It
                                        // is possible to add the authority to a denylist or  punish the byzantine authority.
                                        warn!(name=?name.concise(), "Bad stake from validator: {:?}", err);
                                        self.data.remove(name);
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
    pub fn is_quorum_reached(&self) -> bool {
        matches!(self, Self::QuorumReached(..))
    }
}

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

    pub fn unique_key_count(&self) -> usize {
        self.stake_maps.len()
    }

    pub fn total_votes(&self) -> StakeUnit {
        self.stake_maps
            .values()
            .map(|(_, stake_aggregator)| stake_aggregator.total_votes())
            .sum()
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
            .map(|(k, (_, s))| (k.clone(), (s.data.keys().copied().collect(), s.total_votes)))
            .collect()
    }
}
