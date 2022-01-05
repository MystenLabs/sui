// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use ed25519_dalek::PublicKey;
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
            .map(|(addr, _)| {
                (
                    *addr,
                    PublicKey::from_bytes(addr.as_ref()).expect("Invalid Authority Key"),
                )
            })
            .collect();
        Committee {
            voting_rights,
            total_votes,
            expanded_keys,
        }
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

    /// Find the highest value than is supported by a quorum of authorities.
    pub fn get_strong_majority_lower_bound<V>(&self, mut values: Vec<(AuthorityName, V)>) -> V
    where
        V: Default + std::cmp::Ord,
    {
        values.sort_by(|(_, x), (_, y)| V::cmp(y, x));
        // Browse values by decreasing order, while tracking how many votes they have.
        let mut score = 0;
        for (name, value) in values {
            score += self.weight(&name);
            if score >= self.quorum_threshold() {
                return value;
            }
        }
        V::default()
    }
}
