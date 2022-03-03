// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use ed25519_dalek::PublicKey;
use rand::distributions::{Distribution, Uniform};
use rand::rngs::OsRng;
use std::collections::{BTreeMap, HashMap};

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Committee {
    pub voting_rights: BTreeMap<AuthorityName, usize>,
    pub total_votes: usize,
    pub expanded_keys: HashMap<AuthorityName, PublicKey>,
}

impl Committee {
    pub fn new(voting_rights: BTreeMap<AuthorityName, usize>) -> Self {
        let total_votes = voting_rights.iter().map(|(_, votes)| votes).sum();
        let expanded_keys: HashMap<_, _> = voting_rights
            .iter()
            .map(|(addr, _)| (*addr, (*addr).try_into().expect("Invalid Authority Key")))
            .collect();
        Committee {
            voting_rights,
            total_votes,
            expanded_keys,
        }
    }

    /// Samples authorities by weight
    pub fn sample(&self) -> &AuthorityName {
        // Uniform number [0, total_votes) non-inclusive of the upper bound
        let between = Uniform::from(0..self.total_votes);
        // OsRng implements CryptoRng and is secure
        let mut _random = between.sample(&mut OsRng);
        for (auth, weight) in &self.voting_rights {
            if *weight > _random {
                return auth;
            }
            _random -= *weight;
        }
        unreachable!();
    }

    pub fn weight(&self, author: &AuthorityName) -> usize {
        *self.voting_rights.get(author).unwrap_or(&0)
    }

    pub fn quorum_threshold(&self) -> usize {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (2 N + 3) / 3 = 2f + 1 + (2k + 2)/3 = 2f + 1 + k = N - f
        2 * self.total_votes / 3 + 1
    }

    pub fn validity_threshold(&self) -> usize {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (N + 2) / 3 = f + 1 + k/3 = f + 1
        (self.total_votes + 2) / 3
    }
}
