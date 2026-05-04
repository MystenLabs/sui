// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_types::committee::Committee;

pub(crate) struct EpochCache {
    // Committees for every epoch in `[start_epoch, current_epoch_number)`.
    // `completed_committees[i]` is the committee for epoch `start_epoch + i`.
    completed_committees: Vec<Arc<Committee>>,
    start_epoch: u64,
    current_epoch_number: u64,
    current_committee: Arc<Committee>,
}

impl EpochCache {
    pub fn new(starting_committee: Committee) -> Self {
        let start_epoch = starting_committee.epoch();
        Self {
            completed_committees: vec![],
            start_epoch,
            current_epoch_number: start_epoch,
            current_committee: Arc::new(starting_committee),
        }
    }

    /// Returns the committee for `epoch`, or `None` if it's outside the
    /// `[start_epoch, current_epoch_number]` range we've ratcheted into.
    pub fn get_committee_for_epoch(&self, epoch: u64) -> Option<Arc<Committee>> {
        if epoch == self.current_epoch_number {
            return Some(self.current_committee.clone());
        }
        if epoch < self.start_epoch {
            return None;
        }
        let idx = (epoch - self.start_epoch) as usize;
        self.completed_committees.get(idx).cloned()
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch_number
    }

    pub fn current_committee(&self) -> &Arc<Committee> {
        &self.current_committee
    }

    pub fn apply_ratchet_update(&mut self, new_committee: Committee) {
        let old_committee = std::mem::replace(&mut self.current_committee, Arc::new(new_committee));
        self.completed_committees.push(old_committee);
        self.current_epoch_number += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_committee(epoch: u64) -> Committee {
        let (committee, _) = Committee::new_simple_test_committee();
        let voting_rights = committee.voting_rights.into_iter().collect();
        Committee::new(epoch, voting_rights)
    }

    #[test]
    fn lookup_works_across_many_epochs() {
        let mut cache = EpochCache::new(make_committee(0));
        for epoch in 1..=5 {
            cache.apply_ratchet_update(make_committee(epoch));
        }

        assert_eq!(cache.current_epoch(), 5);

        for epoch in 0..=5 {
            let committee = cache.get_committee_for_epoch(epoch).unwrap();
            assert_eq!(committee.epoch(), epoch);
        }

        assert!(cache.get_committee_for_epoch(6).is_none());
    }

    #[test]
    fn lookup_works_with_non_zero_start_epoch() {
        let mut cache = EpochCache::new(make_committee(1029));
        for epoch in 1030..=1032 {
            cache.apply_ratchet_update(make_committee(epoch));
        }

        assert_eq!(cache.current_epoch(), 1032);

        for epoch in 1029..=1032 {
            let committee = cache.get_committee_for_epoch(epoch).unwrap();
            assert_eq!(committee.epoch(), epoch);
        }

        // Epochs before the start are not retrievable.
        assert!(cache.get_committee_for_epoch(0).is_none());
        assert!(cache.get_committee_for_epoch(1028).is_none());
        // Epochs past current haven't been ratcheted into yet.
        assert!(cache.get_committee_for_epoch(1033).is_none());
    }
}
