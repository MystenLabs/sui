// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, marker::PhantomData};

use consensus_config::{AuthorityIndex, Committee, Stake};

pub(crate) trait CommitteeThreshold {
    fn is_threshold(committee: &Committee, amount: Stake) -> bool;
}

pub(crate) struct QuorumThreshold;

#[allow(unused)]
pub(crate) struct ValidityThreshold;

impl CommitteeThreshold for QuorumThreshold {
    fn is_threshold(committee: &Committee, amount: Stake) -> bool {
        committee.reached_quorum(amount)
    }
}

impl CommitteeThreshold for ValidityThreshold {
    fn is_threshold(committee: &Committee, amount: Stake) -> bool {
        committee.reached_validity(amount)
    }
}

pub(crate) struct StakeAggregator<T> {
    votes: BTreeSet<AuthorityIndex>,
    stake: Stake,
    _phantom: PhantomData<T>,
}

impl<T: CommitteeThreshold> StakeAggregator<T> {
    pub(crate) fn new() -> Self {
        Self {
            votes: Default::default(),
            stake: 0,
            _phantom: Default::default(),
        }
    }

    /// Adds a vote for the specified authority index to the aggregator. It is guaranteed to count
    /// the vote only once for an authority. The method returns true when the required threshold has
    /// been reached.
    pub(crate) fn add(&mut self, vote: AuthorityIndex, committee: &Committee) -> bool {
        if self.votes.insert(vote) {
            self.stake += committee.stake(vote);
        }
        T::is_threshold(committee, self.stake)
    }

    pub(crate) fn stake(&self) -> Stake {
        self.stake
    }

    pub(crate) fn reached_threshold(&self, committee: &Committee) -> bool {
        T::is_threshold(committee, self.stake)
    }

    pub(crate) fn clear(&mut self) {
        self.votes.clear();
        self.stake = 0;
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::{local_committee_and_keys, AuthorityIndex};

    use super::*;

    #[test]
    fn test_aggregator_quorum_threshold() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<QuorumThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(!aggregator.add(AuthorityIndex::new_for_test(1), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(2), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(3), &committee));
    }

    #[test]
    fn test_aggregator_validity_threshold() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<ValidityThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));
    }

    #[test]
    fn test_aggregator_clear() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let mut aggregator = StakeAggregator::<ValidityThreshold>::new();

        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));

        // clear the aggregator
        aggregator.clear();

        // now add them again
        assert!(!aggregator.add(AuthorityIndex::new_for_test(0), &committee));
        assert!(aggregator.add(AuthorityIndex::new_for_test(1), &committee));
    }
}
