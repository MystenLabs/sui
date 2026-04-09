// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_types::committee::Committee;

pub(crate) struct EpochCache {
    completed_committees: Vec<Arc<Committee>>,
    current_epoch_number: u64,
    current_committee: Arc<Committee>,
}

impl EpochCache {
    pub fn new(genesis_committee: Committee) -> Self {
        Self {
            completed_committees: vec![],
            current_epoch_number: 0,
            current_committee: Arc::new(genesis_committee),
        }
    }

    /// Returns the committee for `epoch`, or `None` if the cache hasn't ratcheted that far.
    /// Relies on the invariant that the cache starts at epoch 0 and `apply_ratchet_update`
    /// pushes completed epochs in order, so `completed_committees[N]` is epoch `N`.
    pub fn get_committee_for_epoch(&self, epoch: u64) -> Option<Arc<Committee>> {
        if epoch == self.current_epoch_number {
            return Some(self.current_committee.clone());
        }
        self.completed_committees.get(epoch as usize).cloned()
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
}
