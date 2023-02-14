// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfo};
use sui_types::error::SuiError;
use sui_types::message_envelope::{Envelope, Message, VerifiedEnvelope};

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
impl<S: Clone, const STRENGTH: bool> StakeAggregator<S, STRENGTH> {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            data: Default::default(),
            total_votes: Default::default(),
            committee,
        }
    }

    pub fn from_iter<I: Iterator<Item = (AuthorityName, S)>>(
        committee: Arc<Committee>,
        data: I,
    ) -> Self {
        let mut this = Self::new(committee);
        for (authority, s) in data {
            this.insert_generic(authority, s);
        }
        this
    }

    /// A generic version of inserting arbitrary type of V (e.g. void type).
    /// If V is AuthoritySignInfo, the `insert` function should be used instead since it does extra
    /// checks and aggregations in the end.
    pub fn insert_generic(&mut self, authority: AuthorityName, s: S) -> InsertResult<S, ()> {
        match self.data.entry(authority) {
            Entry::Occupied(oc) => {
                return InsertResult::RepeatingEntry {
                    previous: oc.get().clone(),
                    new: s,
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
                InsertResult::QuorumReached(())
            } else {
                InsertResult::NotEnoughVotes
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
}

impl<const STRENGTH: bool> StakeAggregator<AuthoritySignInfo, STRENGTH> {
    /// Insert an authority signature. This is the primary way to use the aggregator and a few
    /// dedicated checks are performed to make sure things work.
    /// If quorum is reached, we return AuthorityQuorumSignInfo directly.
    pub fn insert(
        &mut self,
        sig: AuthoritySignInfo,
    ) -> InsertResult<AuthoritySignInfo, AuthorityQuorumSignInfo<STRENGTH>> {
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
                    Ok(aggregated) => InsertResult::QuorumReached(aggregated),
                    Err(error) => InsertResult::Failed { error },
                }
            }
            // The following is necessary to change the template type of InsertResult.
            InsertResult::RepeatingEntry { previous, new } => {
                InsertResult::RepeatingEntry { previous, new }
            }
            InsertResult::Failed { error } => InsertResult::Failed { error },
            InsertResult::NotEnoughVotes => InsertResult::NotEnoughVotes,
        }
    }
}

pub enum InsertResult<V, CertT> {
    QuorumReached(CertT),
    RepeatingEntry { previous: V, new: V },
    Failed { error: SuiError },
    NotEnoughVotes,
}

impl<V, CertT> InsertResult<V, CertT> {
    pub fn is_quorum_reached(&self) -> bool {
        matches!(self, Self::QuorumReached(..))
    }
}

/// MultiStakeAggregator is a utility data structure that tracks the stake accumulation of
/// potentially multiple different values (usually due to byzantine/corrupted responses). Each
/// value is tracked using a StakeAggregator and determine whether it has reached a quorum.
/// Once quorum is reached, the `cert` field will be set. This also means there will be only one
/// cert in the end, if any.
/// A specialized implementation is also provided for `Message` value type, so that we could create
/// `Envelope` directly.
#[derive(Debug)]
pub struct MultiStakeAggregator<K, V, const STRENGTH: bool> {
    committee: Arc<Committee>,
    stake_maps: HashMap<K, (V, StakeAggregator<AuthoritySignInfo, STRENGTH>)>,
    cert: Option<(V, AuthorityQuorumSignInfo<STRENGTH>)>,
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH> {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            committee,
            stake_maps: Default::default(),
            cert: None,
        }
    }

    pub fn unique_key_count(&self) -> usize {
        self.stake_maps.len()
    }

    pub fn get_certificate(&self) -> &Option<(V, AuthorityQuorumSignInfo<STRENGTH>)> {
        &self.cert
    }
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH>
where
    K: Hash + Eq,
    V: Clone,
{
    // TODO: Change this to return the certificate instead.
    pub fn add(&mut self, k: K, v: &V, sig: AuthoritySignInfo) -> bool {
        let entry = self
            .stake_maps
            .entry(k)
            .or_insert((v.clone(), StakeAggregator::new(self.committee.clone())));
        match entry.1.insert(sig) {
            InsertResult::QuorumReached(cert) => {
                self.cert = Some((v.clone(), cert));
                true
            }
            _ => false,
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
                    (
                        s.data.iter().map(|(name, _)| *name).collect(),
                        s.total_votes,
                    ),
                )
            })
            .collect()
    }
}

impl<K, V, const STRENGTH: bool> MultiStakeAggregator<K, V, STRENGTH>
where
    V: Message + Clone,
{
    pub fn get_message_cert(
        &self,
    ) -> Option<VerifiedEnvelope<V, AuthorityQuorumSignInfo<STRENGTH>>> {
        self.get_certificate().as_ref().map(|(message, sig)| {
            // TODO: This is in fact error prone, and we may want to verify the signature.
            VerifiedEnvelope::new_unchecked(Envelope::new_from_data_and_sig(
                message.clone(),
                sig.clone(),
            ))
        })
    }
}
