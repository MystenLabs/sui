// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};

pub struct StakeAggregator<V, const STRENGTH: bool> {
    data: HashMap<AuthorityName, V>,
    stake: StakeUnit,
    committee: Committee,
}

impl<V: Clone, const STRENGTH: bool> StakeAggregator<V, STRENGTH> {
    pub fn new(committee: Committee) -> Self {
        Self {
            data: Default::default(),
            stake: Default::default(),
            committee,
        }
    }

    pub fn from_iter<I: Iterator<Item = (AuthorityName, V)>>(
        committee: Committee,
        data: I,
    ) -> Self {
        let mut this = Self::new(committee);
        for (authority, v) in data {
            this.insert(authority, v);
        }
        this
    }

    pub fn insert(&mut self, authority: AuthorityName, v: V) -> InsertResult<V> {
        match self.data.entry(authority) {
            Entry::Occupied(oc) => {
                return InsertResult::RepeatingEntry {
                    previous: oc.get().clone(),
                    new: v,
                };
            }
            Entry::Vacant(va) => {
                va.insert(v);
            }
        }
        self.stake += self.committee.weight(&authority);
        if self.stake >= self.committee.threshold::<STRENGTH>() {
            InsertResult::QuorumReached(&self.data)
        } else {
            InsertResult::NotEnoughVotes
        }
    }

    pub fn contains_key(&self, authority: &AuthorityName) -> bool {
        self.data.contains_key(authority)
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }
}

pub enum InsertResult<'a, V> {
    QuorumReached(&'a HashMap<AuthorityName, V>),
    RepeatingEntry { previous: V, new: V },
    NotEnoughVotes,
}

impl<'a, V> InsertResult<'a, V> {
    pub fn is_quorum_reached(&self) -> bool {
        matches!(self, InsertResult::QuorumReached(_))
    }
}
