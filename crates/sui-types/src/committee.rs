// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use crate::crypto::{random_committee_key_pairs_of_size, AuthorityKeyPair, AuthorityPublicKey};
use crate::error::{SuiError, SuiResult};
use crate::multiaddr::Multiaddr;
use fastcrypto::traits::KeyPair;
use itertools::Itertools;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
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

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
pub struct Committee {
    pub epoch: EpochId,
    pub voting_rights: Vec<(AuthorityName, StakeUnit)>,
    // TODO: Remove and replace with constant.
    pub total_votes: StakeUnit,
    expanded_keys: HashMap<AuthorityName, AuthorityPublicKey>,
    index_map: HashMap<AuthorityName, usize>,
    // TODO: This is no longer needed. This file needs a cleanup since we removed the optional
    // cached expanded keys.
    loaded: bool,
}

impl Committee {
    pub fn new(epoch: EpochId, voting_rights: BTreeMap<AuthorityName, StakeUnit>) -> Self {
        let mut voting_rights: Vec<(AuthorityName, StakeUnit)> =
            voting_rights.iter().map(|(a, s)| (*a, *s)).collect();

        assert!(!voting_rights.is_empty());
        assert!(voting_rights.iter().any(|(_, s)| *s != 0));

        voting_rights.sort_by_key(|(a, _)| *a);
        let total_votes = voting_rights.iter().map(|(_, votes)| *votes).sum();

        let (expanded_keys, index_map) = Self::load_inner(&voting_rights);

        Committee {
            epoch,
            voting_rights,
            total_votes,
            expanded_keys,
            index_map,
            loaded: true,
        }
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

    pub fn reload_fields(&mut self) {
        let (expanded_keys, index_map) = Committee::load_inner(&self.voting_rights);
        self.expanded_keys = expanded_keys;
        self.index_map = index_map;
        self.loaded = true;
    }

    pub fn authority_index(&self, author: &AuthorityName) -> Option<u32> {
        if !self.loaded {
            return self
                .voting_rights
                .iter()
                .position(|(a, _)| a == author)
                .map(|i| i as u32);
        }
        self.index_map.get(author).map(|i| *i as u32)
    }

    pub fn authority_by_index(&self, index: u32) -> Option<&AuthorityName> {
        self.voting_rights.get(index as usize).map(|(name, _)| name)
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

    pub fn shuffle_by_stake(
        &self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
    ) -> Vec<AuthorityName> {
        self.shuffle_by_stake_with_rng(preferences, restrict_to, &mut ThreadRng::default())
    }

    pub fn shuffle_by_stake_with_rng(
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

    pub fn weight(&self, author: &AuthorityName) -> StakeUnit {
        match self.voting_rights.binary_search_by_key(author, |(a, _)| *a) {
            Err(_) => 0,
            Ok(idx) => self.voting_rights[idx].1,
        }
    }

    pub fn quorum_threshold(&self) -> StakeUnit {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (2 N + 3) / 3 = 2f + 1 + (2k + 2)/3 = 2f + 1 + k = N - f
        2 * self.total_votes / 3 + 1
    }

    pub fn validity_threshold(&self) -> StakeUnit {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (N + 2) / 3 = f + 1 + k/3 = f + 1
        validity_threshold(self.total_votes)
    }

    #[inline]
    pub fn threshold<const STRENGTH: bool>(&self) -> StakeUnit {
        if STRENGTH {
            self.quorum_threshold()
        } else {
            self.validity_threshold()
        }
    }

    /// Given a sequence of (AuthorityName, value) for values, provide the
    /// value at the particular threshold by stake. This orders all provided values
    /// in ascending order and pick the appropriate value that has under it threshold
    /// stake. You may use the function `validity_threshold` or `quorum_threshold` to
    /// pick the f+1 (1/3 stake) or 2f+1 (2/3 stake) thresholds respectively.
    ///
    /// This function may be used in a number of settings:
    /// - When we pass in a set of values produced by authorities with at least 2/3 stake
    ///   and pick a validity_threshold it ensures that the resulting value is either itself
    ///   or is in between values provided by an honest node.
    /// - When we pass in values associated with the totality of stake and set a threshold
    ///   of quorum_threshold, we ensure that at least a majority of honest nodes (ie >1/3
    ///   out of the 2/3 threshold) have a value smaller than the value returned.
    pub fn robust_value<A, V>(
        &self,
        items: impl Iterator<Item = (A, V)>,
        threshold: StakeUnit,
    ) -> (AuthorityName, V)
    where
        A: Borrow<AuthorityName> + Ord,
        V: Ord,
    {
        debug_assert!(threshold < self.total_votes);

        let items = items
            .map(|(a, v)| (v, self.weight(a.borrow()), *a.borrow()))
            .sorted();
        let mut total = 0;
        for (v, s, a) in items {
            total += s;
            if threshold <= total {
                return (a, v);
            }
        }
        unreachable!();
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

    // ===== Testing-only methods =====
    //
    pub fn new_simple_test_committee_of_size(size: usize) -> (Self, Vec<AuthorityKeyPair>) {
        let key_pairs: Vec<_> = random_committee_key_pairs_of_size(size)
            .into_iter()
            .collect();
        let committee = Self::new(
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

    /// Generate a simple committee with 4 validators each with equal voting stake of 1.
    pub fn new_simple_test_committee() -> (Self, Vec<AuthorityKeyPair>) {
        Self::new_simple_test_committee_of_size(4)
    }
}

impl PartialEq for Committee {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.voting_rights == other.voting_rights
            && self.total_votes == other.total_votes
    }
}

impl Hash for Committee {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.voting_rights.hash(state);
        self.total_votes.hash(state);
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

pub fn validity_threshold(total_stake: StakeUnit) -> StakeUnit {
    // If N = 3f + 1 + k (0 <= k < 3)
    // then (N + 2) / 3 = f + 1 + k/3 = f + 1
    (total_stake + 2) / 3
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkMetadata {
    pub network_address: Multiaddr,
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

        let committee = Committee::new(0, authorities);

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

    #[test]
    fn test_robust_value() {
        let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
        let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
        let (_, sec3): (_, AuthorityKeyPair) = get_key_pair();
        let (_, sec4): (_, AuthorityKeyPair) = get_key_pair();
        let a1: AuthorityName = sec1.public().into();
        let a2: AuthorityName = sec2.public().into();
        let a3: AuthorityName = sec3.public().into();
        let a4: AuthorityName = sec4.public().into();

        let mut authorities = BTreeMap::new();
        authorities.insert(a1, 1);
        authorities.insert(a2, 1);
        authorities.insert(a3, 1);
        authorities.insert(a4, 1);
        let committee = Committee::new(0, authorities);
        let items = vec![(a1, 666), (a2, 1), (a3, 2), (a4, 0)];
        assert_eq!(
            committee.robust_value(items.into_iter(), committee.quorum_threshold()),
            (a3, 2)
        );

        let items = vec![(a1, "a"), (a2, "b"), (a3, "c"), (a4, "d")];
        assert_eq!(
            committee.robust_value(items.into_iter(), committee.quorum_threshold()),
            (a3, "c")
        );
    }
}
