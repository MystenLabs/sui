// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use mysten_common::debug_fatal;
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, StakeUnit},
};

/// Aggregates various types of statuses from different authorities,
/// and the total stake of authorities that have inserted statuses.
/// Only keeps the latest status for each authority.
pub(crate) struct StatusAggregator<T> {
    committee: Arc<Committee>,
    total_votes: StakeUnit,
    statuses: BTreeMap<AuthorityName, T>,
}

impl<T> StatusAggregator<T> {
    pub(crate) fn new(committee: Arc<Committee>) -> Self {
        Self {
            committee,
            total_votes: 0,
            statuses: BTreeMap::new(),
        }
    }

    /// Returns true if the status is inserted the first time for the authority.
    pub(crate) fn insert(&mut self, authority: AuthorityName, status: T) -> bool {
        let Some(index) = self.committee.authority_index(&authority) else {
            debug_fatal!("Authority {} not found in committee", authority);
            return false;
        };
        if self.statuses.insert(authority, status).is_some() {
            return false;
        }
        self.total_votes += self.committee.stake_by_index(index).unwrap();
        true
    }

    /// Returns the total stake of authorities that have inserted statuses.
    pub(crate) fn total_votes(&self) -> StakeUnit {
        self.total_votes
    }

    /// Returns the status of each authority.
    pub(crate) fn statuses(&self) -> &BTreeMap<AuthorityName, T> {
        &self.statuses
    }

    pub(crate) fn reached_validity_threshold(&self) -> bool {
        self.total_votes >= self.committee.validity_threshold()
    }

    pub(crate) fn reached_quorum_threshold(&self) -> bool {
        self.total_votes >= self.committee.quorum_threshold()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let (committee, _) = Committee::new_simple_test_committee();
        let authorities: Vec<_> = committee.names().copied().collect();
        let stake_per_authority = committee.stake_by_index(0).unwrap();
        let mut aggregator = StatusAggregator::<&str>::new(Arc::new(committee));

        // Insert a status for authority 0.
        let initial_status = "initial";
        assert!(aggregator.insert(authorities[0], initial_status));
        assert_eq!(aggregator.total_votes(), stake_per_authority);
        assert_eq!(
            aggregator.statuses().get(&authorities[0]),
            Some(&initial_status)
        );

        // Insert the same status for authority 0 again.
        assert!(!aggregator.insert(authorities[0], initial_status));
        assert_eq!(aggregator.total_votes(), stake_per_authority);

        // Insert a different status for authority 0.
        let different_status = "different";
        assert!(!aggregator.insert(authorities[0], different_status));
        assert_eq!(aggregator.total_votes(), stake_per_authority);
        assert_eq!(
            aggregator.statuses().get(&authorities[0]),
            Some(&different_status)
        );

        // Does not reach validity threshold or quorum threshold.
        assert!(!aggregator.reached_validity_threshold());
        assert!(!aggregator.reached_quorum_threshold());

        // Insert a new status for authority 1.
        let new_status = "new";
        assert!(aggregator.insert(authorities[1], new_status));
        assert_eq!(aggregator.total_votes(), 2 * stake_per_authority);
        assert_eq!(
            aggregator.statuses().get(&authorities[1]),
            Some(&new_status)
        );

        // Reaches validity threshold but not quorum threshold.
        assert!(aggregator.reached_validity_threshold());
        assert!(!aggregator.reached_quorum_threshold());

        // Insert a new status for authority 2.
        assert!(aggregator.insert(authorities[2], new_status));
        assert_eq!(aggregator.total_votes(), 3 * stake_per_authority);
        assert_eq!(
            aggregator.statuses().get(&authorities[2]),
            Some(&new_status)
        );

        // Reaches validity and quorum thresholds.
        assert!(aggregator.reached_validity_threshold());
        assert!(aggregator.reached_quorum_threshold());
    }
}
