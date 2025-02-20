// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use crate::crypto::{
    random_committee_key_pairs_of_size, AuthorityKeyPair, AuthorityPublicKey, NetworkPublicKey,
};
use crate::error::{SuiError, SuiResult};
use crate::multiaddr::Multiaddr;
use fastcrypto::traits::KeyPair;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use rand::rngs::{StdRng, ThreadRng};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
pub use sui_protocol_config::ProtocolVersion;

pub type EpochId = u64;

// TODO: the stake and voting power of a validator can be different so
// in some places when we are actually referring to the voting power, we
// should use a different type alias, field name, etc.
pub type StakeUnit = u64;

pub type CommitteeDigest = [u8; 32];

// The voting power, quorum threshold and max voting power are defined in the `voting_power.move` module.
// We're following the very same convention in the validator binaries.

/// Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
/// as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%).
/// Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
/// than recomputing these.
pub const TOTAL_VOTING_POWER: StakeUnit = 10_000;

/// Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
/// up to BFT assumptions
pub const QUORUM_THRESHOLD: StakeUnit = 6_667;

/// Validity threshold defined by f+1
pub const VALIDITY_THRESHOLD: StakeUnit = 3_334;

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
pub struct Committee {
    pub epoch: EpochId,
    pub voting_rights: Vec<(AuthorityName, StakeUnit)>,
    expanded_keys: HashMap<AuthorityName, AuthorityPublicKey>,
    index_map: HashMap<AuthorityName, usize>,
}

impl Committee {
    pub fn new(epoch: EpochId, voting_rights: BTreeMap<AuthorityName, StakeUnit>) -> Self {
        let mut voting_rights: Vec<(AuthorityName, StakeUnit)> =
            voting_rights.iter().map(|(a, s)| (*a, *s)).collect();

        assert!(!voting_rights.is_empty());
        assert!(voting_rights.iter().any(|(_, s)| *s != 0));

        voting_rights.sort_by_key(|(a, _)| *a);
        let total_votes: StakeUnit = voting_rights.iter().map(|(_, votes)| *votes).sum();
        assert_eq!(total_votes, TOTAL_VOTING_POWER);

        let (expanded_keys, index_map) = Self::load_inner(&voting_rights);

        Committee {
            epoch,
            voting_rights,
            expanded_keys,
            index_map,
        }
    }

    /// Normalize the given weights to TOTAL_VOTING_POWER and create the committee.
    /// Used for testing only: a production system is using the voting weights
    /// of the Sui System object.
    pub fn new_for_testing_with_normalized_voting_power(
        epoch: EpochId,
        mut voting_weights: BTreeMap<AuthorityName, StakeUnit>,
    ) -> Self {
        let num_nodes = voting_weights.len();
        let total_votes: StakeUnit = voting_weights.values().cloned().sum();

        let normalization_coef = TOTAL_VOTING_POWER as f64 / total_votes as f64;
        let mut total_sum = 0;
        for (idx, (_auth, weight)) in voting_weights.iter_mut().enumerate() {
            if idx < num_nodes - 1 {
                *weight = (*weight as f64 * normalization_coef).floor() as u64; // adjust the weights following the normalization coef
                total_sum += *weight;
            } else {
                // the last element is taking all the rest
                *weight = TOTAL_VOTING_POWER - total_sum;
            }
        }

        Self::new(epoch, voting_weights)
    }

    // We call this if these have not yet been computed
    pub fn load_inner(
        voting_rights: &[(AuthorityName, StakeUnit)],
    ) -> (
        HashMap<AuthorityName, AuthorityPublicKey>,
        HashMap<AuthorityName, usize>,
    ) {
        let expanded_keys: HashMap<AuthorityName, AuthorityPublicKey> = voting_rights
            .iter()
            .map(|(addr, _)| {
                (
                    *addr,
                    (*addr)
                        .try_into()
                        .expect("Validator pubkey is always verified on-chain"),
                )
            })
            .collect();

        let index_map: HashMap<AuthorityName, usize> = voting_rights
            .iter()
            .enumerate()
            .map(|(index, (addr, _))| (*addr, index))
            .collect();
        (expanded_keys, index_map)
    }

    pub fn authority_index(&self, author: &AuthorityName) -> Option<u32> {
        self.index_map.get(author).map(|i| *i as u32)
    }

    pub fn authority_by_index(&self, index: u32) -> Option<&AuthorityName> {
        self.voting_rights.get(index as usize).map(|(name, _)| name)
    }

    pub fn stake_by_index(&self, index: u32) -> Option<StakeUnit> {
        self.voting_rights
            .get(index as usize)
            .map(|(_, stake)| *stake)
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn public_key(&self, authority: &AuthorityName) -> SuiResult<&AuthorityPublicKey> {
        debug_assert_eq!(self.expanded_keys.len(), self.voting_rights.len());
        match self.expanded_keys.get(authority) {
            Some(v) => Ok(v),
            None => Err(SuiError::InvalidCommittee(format!(
                "Authority #{} not found, committee size {}",
                authority,
                self.expanded_keys.len()
            ))),
        }
    }

    /// Samples authorities by weight
    pub fn sample(&self) -> &AuthorityName {
        // unwrap safe unless committee is empty
        Self::choose_multiple_weighted(&self.voting_rights[..], 1, &mut ThreadRng::default())
            .next()
            .unwrap()
    }

    fn choose_multiple_weighted<'a>(
        slice: &'a [(AuthorityName, StakeUnit)],
        count: usize,
        rng: &mut impl Rng,
    ) -> impl Iterator<Item = &'a AuthorityName> {
        // unwrap is safe because we validate the committee composition in `new` above.
        // See https://docs.rs/rand/latest/rand/distributions/weighted/enum.WeightedError.html
        // for possible errors.
        slice
            .choose_multiple_weighted(rng, count, |(_, weight)| *weight as f64)
            .unwrap()
            .map(|(a, _)| a)
    }

    pub fn choose_multiple_weighted_iter(
        &self,
        count: usize,
    ) -> impl Iterator<Item = &AuthorityName> {
        self.voting_rights
            .choose_multiple_weighted(&mut ThreadRng::default(), count, |(_, weight)| {
                *weight as f64
            })
            .unwrap()
            .map(|(a, _)| a)
    }

    pub fn total_votes(&self) -> StakeUnit {
        TOTAL_VOTING_POWER
    }

    pub fn quorum_threshold(&self) -> StakeUnit {
        QUORUM_THRESHOLD
    }

    pub fn validity_threshold(&self) -> StakeUnit {
        VALIDITY_THRESHOLD
    }

    pub fn threshold<const STRENGTH: bool>(&self) -> StakeUnit {
        if STRENGTH {
            QUORUM_THRESHOLD
        } else {
            VALIDITY_THRESHOLD
        }
    }

    pub fn num_members(&self) -> usize {
        self.voting_rights.len()
    }

    pub fn members(&self) -> impl Iterator<Item = &(AuthorityName, StakeUnit)> {
        self.voting_rights.iter()
    }

    pub fn names(&self) -> impl Iterator<Item = &AuthorityName> {
        self.voting_rights.iter().map(|(name, _)| name)
    }

    pub fn stakes(&self) -> impl Iterator<Item = StakeUnit> + '_ {
        self.voting_rights.iter().map(|(_, stake)| *stake)
    }

    pub fn authority_exists(&self, name: &AuthorityName) -> bool {
        self.voting_rights
            .binary_search_by_key(name, |(a, _)| *a)
            .is_ok()
    }

    /// Derive a seed deterministically from the transaction digest and shuffle the validators.
    pub fn shuffle_by_stake_from_tx_digest(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Vec<AuthorityName> {
        // the 32 is as requirement of the default StdRng::from_seed choice
        let digest_bytes = tx_digest.into_inner();

        // permute the validators deterministically, based on the digest
        let mut rng = StdRng::from_seed(digest_bytes);
        self.shuffle_by_stake_with_rng(None, None, &mut rng)
    }

    // ===== Testing-only methods =====
    //
    pub fn new_simple_test_committee_of_size(size: usize) -> (Self, Vec<AuthorityKeyPair>) {
        let key_pairs: Vec<_> = random_committee_key_pairs_of_size(size)
            .into_iter()
            .collect();
        let committee = Self::new_for_testing_with_normalized_voting_power(
            0,
            key_pairs
                .iter()
                .map(|key| {
                    (AuthorityName::from(key.public()), /* voting right */ 1)
                })
                .collect(),
        );
        (committee, key_pairs)
    }

    pub fn new_simple_test_committee_with_normalized_voting_power(
        voting_weights: Vec<StakeUnit>,
    ) -> (Self, Vec<AuthorityKeyPair>) {
        let key_pairs: Vec<_> = random_committee_key_pairs_of_size(voting_weights.len())
            .into_iter()
            .sorted_by_key(|key| key.public().clone())
            .collect();
        let committee = Self::new_for_testing_with_normalized_voting_power(
            0,
            voting_weights
                .iter()
                .enumerate()
                .map(|(idx, weight)| (AuthorityName::from(key_pairs[idx].public()), *weight))
                .collect(),
        );
        (committee, key_pairs)
    }

    /// Generate a simple committee with 4 validators each with equal voting stake of 1.
    pub fn new_simple_test_committee() -> (Self, Vec<AuthorityKeyPair>) {
        Self::new_simple_test_committee_of_size(4)
    }
}

impl CommitteeTrait<AuthorityName> for Committee {
    fn shuffle_by_stake_with_rng(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
        rng: &mut impl Rng,
    ) -> Vec<AuthorityName> {
        let restricted = self
            .voting_rights
            .iter()
            .filter(|(name, _)| {
                if let Some(restrict_to) = restrict_to {
                    restrict_to.contains(name)
                } else {
                    true
                }
            })
            .cloned();

        let (preferred, rest): (Vec<_>, Vec<_>) = if let Some(preferences) = preferences {
            restricted.partition(|(name, _)| preferences.contains(name))
        } else {
            (Vec::new(), restricted.collect())
        };

        Self::choose_multiple_weighted(&preferred, preferred.len(), rng)
            .chain(Self::choose_multiple_weighted(&rest, rest.len(), rng))
            .cloned()
            .collect()
    }

    fn weight(&self, author: &AuthorityName) -> StakeUnit {
        match self.voting_rights.binary_search_by_key(author, |(a, _)| *a) {
            Err(_) => 0,
            Ok(idx) => self.voting_rights[idx].1,
        }
    }
}

impl PartialEq for Committee {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch && self.voting_rights == other.voting_rights
    }
}

impl Hash for Committee {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.voting_rights.hash(state);
    }
}

impl Display for Committee {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut voting_rights = String::new();
        for (name, vote) in &self.voting_rights {
            write!(voting_rights, "{}: {}, ", name.concise(), vote)?;
        }
        write!(
            f,
            "Committee (epoch={:?}, voting_rights=[{}])",
            self.epoch, voting_rights
        )
    }
}

pub trait CommitteeTrait<K: Ord> {
    fn shuffle_by_stake_with_rng(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<K>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<K>>,
        rng: &mut impl Rng,
    ) -> Vec<K>;

    fn shuffle_by_stake(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<K>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<K>>,
    ) -> Vec<K> {
        self.shuffle_by_stake_with_rng(preferences, restrict_to, &mut ThreadRng::default())
    }

    fn weight(&self, author: &K) -> StakeUnit;
}

#[derive(Clone, Debug)]
pub struct NetworkMetadata {
    pub network_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub network_public_key: Option<NetworkPublicKey>,
}

#[derive(Clone, Debug)]
pub struct CommitteeWithNetworkMetadata {
    epoch_id: EpochId,
    validators: BTreeMap<AuthorityName, (StakeUnit, NetworkMetadata)>,
    committee: OnceCell<Committee>,
}

impl CommitteeWithNetworkMetadata {
    pub fn new(
        epoch_id: EpochId,
        validators: BTreeMap<AuthorityName, (StakeUnit, NetworkMetadata)>,
    ) -> Self {
        Self {
            epoch_id,
            validators,
            committee: OnceCell::new(),
        }
    }
    pub fn epoch(&self) -> EpochId {
        self.epoch_id
    }

    pub fn validators(&self) -> &BTreeMap<AuthorityName, (StakeUnit, NetworkMetadata)> {
        &self.validators
    }

    pub fn committee(&self) -> &Committee {
        self.committee.get_or_init(|| {
            Committee::new(
                self.epoch_id,
                self.validators
                    .iter()
                    .map(|(name, (stake, _))| (*name, *stake))
                    .collect(),
            )
        })
    }
}

impl Display for CommitteeWithNetworkMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CommitteeWithNetworkMetadata (epoch={}, validators={:?})",
            self.epoch_id, self.validators
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::{get_key_pair, AuthorityKeyPair};
    use fastcrypto::traits::KeyPair;

    #[test]
    fn test_shuffle_by_weight() {
        let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
        let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
        let (_, sec3): (_, AuthorityKeyPair) = get_key_pair();
        let a1: AuthorityName = sec1.public().into();
        let a2: AuthorityName = sec2.public().into();
        let a3: AuthorityName = sec3.public().into();

        let mut authorities = BTreeMap::new();
        authorities.insert(a1, 1);
        authorities.insert(a2, 1);
        authorities.insert(a3, 1);

        let committee = Committee::new_for_testing_with_normalized_voting_power(0, authorities);

        assert_eq!(committee.shuffle_by_stake(None, None).len(), 3);

        let mut pref = BTreeSet::new();
        pref.insert(a2);

        // preference always comes first
        for _ in 0..100 {
            assert_eq!(
                a2,
                *committee
                    .shuffle_by_stake(Some(&pref), None)
                    .first()
                    .unwrap()
            );
        }

        let mut restrict = BTreeSet::new();
        restrict.insert(a2);

        for _ in 0..100 {
            let res = committee.shuffle_by_stake(None, Some(&restrict));
            assert_eq!(1, res.len());
            assert_eq!(a2, res[0]);
        }

        // empty preferences are valid
        let res = committee.shuffle_by_stake(Some(&BTreeSet::new()), None);
        assert_eq!(3, res.len());

        let res = committee.shuffle_by_stake(None, Some(&BTreeSet::new()));
        assert_eq!(0, res.len());
    }
}
