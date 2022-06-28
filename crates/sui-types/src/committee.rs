// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use crate::error::SuiResult;
use ed25519_dalek::PublicKey;
use itertools::Itertools;
use rand_latest::rngs::OsRng;
use rand_latest::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};

pub type EpochId = u64;

pub type StakeUnit = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Committee {
    pub epoch: EpochId,
    pub voting_rights: Vec<(AuthorityName, StakeUnit)>,
    pub total_votes: StakeUnit,
    // Note: this is a derived structure, no need to store.
    #[serde(skip)]
    expanded_keys: HashMap<AuthorityName, PublicKey>,
}

impl Committee {
    pub fn new(epoch: EpochId, voting_rights: BTreeMap<AuthorityName, StakeUnit>) -> Self {
        let mut voting_rights: Vec<(AuthorityName, StakeUnit)> =
            voting_rights.iter().map(|(a, s)| (*a, *s)).collect();

        voting_rights.sort_by_key(|(a, _)| *a);
        let total_votes = voting_rights.iter().map(|(_, votes)| *votes).sum();
        let expanded_keys: HashMap<_, _> = voting_rights
            .iter()
            // TODO: Verify all code path to make sure we always have valid public keys.
            // e.g. when a new validator is registering themself on-chain.
            .map(|(addr, _)| (*addr, (*addr).try_into().expect("Invalid Authority Key")))
            .collect();
        Committee {
            epoch,
            voting_rights,
            total_votes,
            expanded_keys,
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn public_key(&self, authority: &AuthorityName) -> SuiResult<PublicKey> {
        match self.expanded_keys.get(authority) {
            Some(v) => Ok(*v),
            None => (*authority).try_into(),
        }
    }

    /// Samples authorities by weight
    pub fn sample(&self) -> &AuthorityName {
        // unwrap safe unless committee is empty
        self.choose_multiple_weighted(1).next().unwrap()
    }

    fn choose_multiple_weighted(&self, count: usize) -> impl Iterator<Item = &AuthorityName> {
        self.voting_rights
            .as_slice()
            .choose_multiple_weighted(&mut OsRng, count, |(_, weight)| *weight as f64)
            .unwrap()
            .map(|(a, _)| a)
    }

    pub fn shuffle_by_stake(&self) -> impl Iterator<Item = &AuthorityName> {
        self.choose_multiple_weighted(self.voting_rights.len())
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
        (self.total_votes + 2) / 3
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
            if threshold < total {
                return (a, v);
            }
        }
        unreachable!();
    }

    pub fn names(&self) -> impl Iterator<Item = &AuthorityName> {
        self.voting_rights.iter().map(|(name, _)| name)
    }

    pub fn stakes(&self) -> impl Iterator<Item = StakeUnit> + '_ {
        self.voting_rights.iter().map(|(_, stake)| *stake)
    }

    pub fn authority_exists(&self, name: &AuthorityName) -> bool {
        match self.voting_rights.binary_search_by_key(name, |(a, _)| *a) {
            Err(_) => false,
            Ok(_) => true,
        }
    }
}

impl PartialEq for Committee {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.voting_rights == other.voting_rights
            && self.total_votes == other.total_votes
    }
}
