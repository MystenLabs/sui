// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};

pub struct StakeAggregator<V, const STRENGTH: bool> {
    data: HashMap<AuthorityName, V>,
    stake: StakeUnit,
    committee: Arc<Committee>,
}

impl<V: Clone, const STRENGTH: bool> StakeAggregator<V, STRENGTH> {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self {
            data: Default::default(),
            stake: Default::default(),
            committee,
        }
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
            InsertResult::Success(&self.data)
        } else {
            InsertResult::NotEnoughVotes
        }
    }

    #[allow(dead_code)]
    pub fn contains_key(&self, authority: &AuthorityName) -> bool {
        self.data.contains_key(authority)
    }

    pub fn committee(&self) -> &Arc<Committee> {
        &self.committee
    }
}

pub enum InsertResult<'a, V> {
    Success(&'a HashMap<AuthorityName, V>),
    RepeatingEntry { previous: V, new: V },
    NotEnoughVotes,
}

impl<'a, V> InsertResult<'a, V> {
    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        matches!(self, InsertResult::Success(_))
    }
}
