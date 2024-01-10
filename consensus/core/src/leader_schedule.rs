// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{prelude::SliceRandom, rngs::StdRng, SeedableRng};
use std::sync::Arc;

use consensus_config::{AuthorityIndex, Committee};

/// The LeaderSchedule is responsible for producing the leader schedule across
/// an epoch. For now it is a simple wrapper around committee to provide a leader
/// for a round deterministically.
// TODO: complete full leader schedule changes
#[derive(Clone)]
pub struct LeaderSchedule {
    committee: Arc<Committee>,
}

#[allow(unused)]
impl LeaderSchedule {
    pub fn new(committee: Arc<Committee>) -> Self {
        Self { committee }
    }

    pub fn elect_leader(&self, round: u32, leader_offset: u32) -> AuthorityIndex {
        cfg_if::cfg_if! {
            // TODO: we need to differentiate the leader strategy in tests, so for
            // some type of testing (ex sim tests) we can use the staked approach.
            if #[cfg(test)] {
                AuthorityIndex::new((round + leader_offset) % self.committee.size() as u32)
            } else {
                self.elect_leader_stake_based(round, leader_offset)
            }
        }
    }

    pub fn elect_leader_stake_based(&self, round: u32, offset: u32) -> AuthorityIndex {
        assert!((offset as usize) < self.committee.size());

        // TODO: this needs to be removed.
        // if genesis, always return index 0
        if round == 0 {
            return AuthorityIndex::new(0);
        }

        // To ensure that we elect different leaders for the same round (using
        // different offset) we are using the round number as seed to shuffle in
        // a weighted way the results, but skip based on the offset.
        // TODO: use a cache in case this proves to be computationally expensive
        let mut seed_bytes = [0u8; 32];
        seed_bytes[32 - 8..].copy_from_slice(&(round).to_le_bytes());
        let mut rng = StdRng::from_seed(seed_bytes);

        let choices = self
            .committee
            .authorities()
            .map(|(index, authority)| (index, authority.stake as f32))
            .collect::<Vec<_>>();

        let leader_index = *choices
            .choose_multiple_weighted(&mut rng, self.committee.size(), |item| item.1)
            .expect("Weighted choice error: stake values incorrect!")
            .skip(offset as usize)
            .map(|(index, _)| index)
            .next()
            .unwrap();

        leader_index
    }
}
