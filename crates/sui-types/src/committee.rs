// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use crate::crypto::{
    random_committee_key_pairs, AuthorityKeyPair, AuthorityPublicKey, NetworkPublicKey,
};
use crate::error::{SuiError, SuiResult};
use fastcrypto::traits::KeyPair;
use multiaddr::Multiaddr;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
pub use sui_protocol_config::ProtocolVersion;

pub type EpochId = u64;

pub type VoteUnit = u64;

pub type CommitteeDigest = [u8; 32];

// The voting power, quorum threshold and max voting power are defined in the `voting_power.move` module.
// We're following the very same convention in the validator binaries.

/// Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
/// as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%).
/// Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
/// than recomputing these.
pub const TOTAL_VOTING_POWER: VoteUnit = 10_000;

/// Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
/// up to BFT assumptions
pub const QUORUM_THRESHOLD: VoteUnit = 6_667;

/// Validity threshold defined by f+1
pub const VALIDITY_THRESHOLD: VoteUnit = 3_334;

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
pub struct Committee {
    pub epoch: EpochId,
    pub voting_weights: Vec<(AuthorityName, VoteUnit)>,
    #[serde(skip)]
    expanded_keys: HashMap<AuthorityName, AuthorityPublicKey>,
    #[serde(skip)]
    index_map: HashMap<AuthorityName, usize>,
    #[serde(skip)]
    loaded: bool,
}

impl Committee {
    pub fn new(
        epoch: EpochId,
        voting_weights: BTreeMap<AuthorityName, VoteUnit>,
    ) -> SuiResult<Self> {
        let mut voting_weights: Vec<(AuthorityName, VoteUnit)> =
            voting_weights.iter().map(|(a, s)| (*a, *s)).collect();

        fp_ensure!(
            // Actual committee size is enforced in sui_system.move.
            // This is just to ensure that choose_multiple_weighted can't fail.
            voting_weights.len() < u32::MAX.try_into().unwrap(),
            SuiError::InvalidCommittee("committee has too many members".into())
        );

        fp_ensure!(
            !voting_weights.is_empty(),
            SuiError::InvalidCommittee("committee has 0 members".into())
        );

        fp_ensure!(
            voting_weights.iter().any(|(_, s)| *s != 0),
            SuiError::InvalidCommittee(
                "at least one committee member must have non-zero voting power.".into()
            )
        );

        voting_weights.sort_by_key(|(a, _)| *a);

        let total_votes: VoteUnit = voting_weights.iter().map(|(_, votes)| *votes).sum();
        fp_ensure!(
            total_votes == TOTAL_VOTING_POWER,
            SuiError::InvalidCommittee(format!(
                "total voting power of a committee is {}, must be {}",
                total_votes, TOTAL_VOTING_POWER
            ))
        );

        let (expanded_keys, index_map) = Self::load_inner(&voting_weights);

        Ok(Committee {
            epoch,
            voting_weights,
            expanded_keys,
            index_map,
            loaded: true,
        })
    }

    /// Normalize the given weights to TOTAL_VOTING_POWER and create the committee.
    /// Used for testing only: a production system is using the voting weights
    /// of the Sui System object.
    pub fn normalize_from_weights_for_testing(
        epoch: EpochId,
        mut voting_weights: BTreeMap<AuthorityName, VoteUnit>,
    ) -> SuiResult<Self> {
        fp_ensure!(
            !voting_weights.is_empty(),
            SuiError::InvalidCommittee("committee has 0 members".into())
        );

        let num_nodes = voting_weights.len();
        let total_votes: VoteUnit = voting_weights.iter().map(|(_, votes)| *votes).sum();

        fp_ensure!(
            total_votes != 0,
            SuiError::InvalidCommittee(
                "at least one committee member must have non-zero voting power.".into()
            )
        );
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
        voting_weights: &[(AuthorityName, VoteUnit)],
    ) -> (
        HashMap<AuthorityName, AuthorityPublicKey>,
        HashMap<AuthorityName, usize>,
    ) {
        let expanded_keys: HashMap<AuthorityName, AuthorityPublicKey> = voting_weights
            .iter()
            // TODO: Verify all code path to make sure we always have valid public keys.
            // e.g. when a new validator is registering themself on-chain.
            .map(|(addr, _)| (*addr, (*addr).try_into().expect("Invalid Authority Key")))
            .collect();

        let index_map: HashMap<AuthorityName, usize> = voting_weights
            .iter()
            .enumerate()
            .map(|(index, (addr, _))| (*addr, index))
            .collect();
        (expanded_keys, index_map)
    }

    pub fn reload_fields(&mut self) {
        let (expanded_keys, index_map) = Committee::load_inner(&self.voting_weights);
        self.expanded_keys = expanded_keys;
        self.index_map = index_map;
        self.loaded = true;
    }

    pub fn authority_index(&self, author: &AuthorityName) -> Option<u32> {
        if !self.loaded {
            return self
                .voting_weights
                .iter()
                .position(|(a, _)| a == author)
                .map(|i| i as u32);
        }
        self.index_map.get(author).map(|i| *i as u32)
    }

    pub fn authority_by_index(&self, index: u32) -> Option<&AuthorityName> {
        self.voting_weights
            .get(index as usize)
            .map(|(name, _)| name)
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn public_key(&self, authority: &AuthorityName) -> SuiResult<AuthorityPublicKey> {
        match self.expanded_keys.get(authority) {
            // TODO: Check if this is unnecessary copying.
            Some(v) => Ok(v.clone()),
            None => (*authority).try_into().map_err(|_| {
                SuiError::InvalidCommittee(format!("Authority #{} not found", authority))
            }),
        }
    }

    /// Samples authorities by weight
    pub fn sample(&self) -> &AuthorityName {
        // unwrap safe unless committee is empty
        Self::choose_multiple_weighted(&self.voting_weights[..], 1, &mut ThreadRng::default())
            .next()
            .unwrap()
    }

    fn choose_multiple_weighted<'a>(
        slice: &'a [(AuthorityName, VoteUnit)],
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

    pub fn shuffle_by_weight(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
    ) -> Vec<AuthorityName> {
        self.shuffle_by_weight_with_rng(preferences, restrict_to, &mut ThreadRng::default())
    }

    pub fn shuffle_by_weight_with_rng(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
        rng: &mut impl Rng,
    ) -> Vec<AuthorityName> {
        let restricted = self
            .voting_weights
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

    pub fn weight(&self, author: &AuthorityName) -> VoteUnit {
        match self
            .voting_weights
            .binary_search_by_key(author, |(a, _)| *a)
        {
            Err(_) => 0,
            Ok(idx) => self.voting_weights[idx].1,
        }
    }

    #[inline]
    pub fn threshold<const STRENGTH: bool>(&self) -> VoteUnit {
        if STRENGTH {
            QUORUM_THRESHOLD
        } else {
            VALIDITY_THRESHOLD
        }
    }

    pub fn num_members(&self) -> usize {
        self.voting_weights.len()
    }

    pub fn members(&self) -> impl Iterator<Item = &(AuthorityName, VoteUnit)> {
        self.voting_weights.iter()
    }

    pub fn names(&self) -> impl Iterator<Item = &AuthorityName> {
        self.voting_weights.iter().map(|(name, _)| name)
    }

    pub fn voting_rights(&self) -> impl Iterator<Item = VoteUnit> + '_ {
        self.voting_weights
            .iter()
            .map(|(_, voting_power)| *voting_power)
    }

    pub fn authority_exists(&self, name: &AuthorityName) -> bool {
        self.voting_weights
            .binary_search_by_key(name, |(a, _)| *a)
            .is_ok()
    }

    // ===== Testing-only methods =====

    /// Generate a simple committee with 4 validators each with equal voting power of 2_500.
    pub fn new_simple_test_committee() -> (Self, Vec<AuthorityKeyPair>) {
        let key_pairs: Vec<_> = random_committee_key_pairs().into_iter().collect();
        let committee = Self::new(
            0,
            key_pairs
                .iter()
                .map(|key| {
                    (
                        AuthorityName::from(key.public()),
                        /* voting right */ 2_500,
                    )
                })
                .collect(),
        )
        .unwrap();
        (committee, key_pairs)
    }
}

impl PartialEq for Committee {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch && self.voting_weights == other.voting_weights
    }
}

impl Hash for Committee {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.voting_weights.hash(state);
    }
}

impl Display for Committee {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut voting_rights = String::new();
        for (name, vote) in &self.voting_weights {
            write!(voting_rights, "{}: {}, ", name.concise(), vote)?;
        }
        write!(
            f,
            "Committee (epoch={:?}, voting_rights=[{}])",
            self.epoch, voting_rights
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkMetadata {
    pub network_pubkey: NetworkPublicKey,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitteeWithNetworkMetadata {
    pub committee: Committee,
    pub network_metadata: BTreeMap<AuthorityName, NetworkMetadata>,
}

impl CommitteeWithNetworkMetadata {
    pub fn epoch(&self) -> EpochId {
        self.committee.epoch()
    }
}

impl Display for CommitteeWithNetworkMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CommitteeWithNetworkMetadata (committee={}, network_metadata={:?})",
            self.committee, self.network_metadata
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

        let committee = Committee::normalize_from_weights_for_testing(0, authorities).unwrap();

        assert_eq!(committee.shuffle_by_weight(None, None).len(), 3);

        let mut pref = BTreeSet::new();
        pref.insert(a2);

        // preference always comes first
        for _ in 0..100 {
            assert_eq!(
                a2,
                *committee
                    .shuffle_by_weight(Some(&pref), None)
                    .first()
                    .unwrap()
            );
        }

        let mut restrict = BTreeSet::new();
        restrict.insert(a2);

        for _ in 0..100 {
            let res = committee.shuffle_by_weight(None, Some(&restrict));
            assert_eq!(1, res.len());
            assert_eq!(a2, res[0]);
        }

        // empty preferences are valid
        let res = committee.shuffle_by_weight(Some(&BTreeSet::new()), None);
        assert_eq!(3, res.len());

        let res = committee.shuffle_by_weight(None, Some(&BTreeSet::new()));
        assert_eq!(0, res.len());
    }
}
