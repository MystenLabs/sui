// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::{AuthorityQuorumSignInfo, AuthoritySignInfo};
use sui_types::error::SuiError;

pub struct StakeAggregator<V, const STRENGTH: bool> {
    data: HashMap<AuthorityName, V>,
    total_votes: StakeUnit,
    committee: Committee,
}

impl<V: Clone, const STRENGTH: bool> StakeAggregator<V, STRENGTH> {
    pub fn new(committee: Committee) -> Self {
        Self {
            data: Default::default(),
            total_votes: Default::default(),
            committee,
        }
    }

    pub fn from_iter<I: Iterator<Item = (AuthorityName, V)>>(
        committee: Committee,
        data: I,
    ) -> Self {
        let mut this = Self::new(committee);
        for (authority, v) in data {
            this.insert_generic(authority, v);
        }
        this
    }

    /// A generic version of inserting arbitrary type of V (e.g. void type).
    /// If V is AuthoritySignInfo, the `insert` function should be used instead since it does extra
    /// checks and aggregations in the end.
    pub fn insert_generic(&mut self, authority: AuthorityName, v: V) -> InsertResult<V, STRENGTH> {
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
        let votes = self.committee.weight(&authority);
        if votes > 0 {
            self.total_votes += votes;
            if self.total_votes >= self.committee.threshold::<STRENGTH>() {
                InsertResult::QuorumReached
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
    pub fn insert(&mut self, sig: AuthoritySignInfo) -> InsertResult<AuthoritySignInfo, STRENGTH> {
        if self.committee.epoch != sig.epoch {
            return InsertResult::Failed {
                error: SuiError::WrongEpoch {
                    expected_epoch: self.committee.epoch,
                    actual_epoch: sig.epoch,
                },
            };
        }
        let result = self.insert_generic(sig.authority, sig);
        if result.is_quorum_reached() {
            match AuthorityQuorumSignInfo::<STRENGTH>::new_from_auth_sign_infos(
                self.data.values().cloned().collect(),
                self.committee(),
            ) {
                Ok(aggregated) => InsertResult::QuorumReachedWithCert(aggregated),
                Err(error) => InsertResult::Failed { error },
            }
        } else {
            result
        }
    }
}

pub enum InsertResult<V, const STRENGTH: bool> {
    QuorumReached,
    QuorumReachedWithCert(AuthorityQuorumSignInfo<STRENGTH>),
    RepeatingEntry { previous: V, new: V },
    Failed { error: SuiError },
    NotEnoughVotes,
}

impl<V, const STRENGTH: bool> InsertResult<V, STRENGTH> {
    pub fn is_quorum_reached(&self) -> bool {
        matches!(self, Self::QuorumReached | Self::QuorumReachedWithCert(_))
    }
}
