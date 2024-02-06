// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::{prelude::SliceRandom, rngs::StdRng, SeedableRng};

use consensus_config::AuthorityIndex;

use crate::context::Context;

/// The LeaderSchedule is responsible for producing the leader schedule across
/// an epoch. For now it is a simple wrapper around Context to provide a leader
/// for a round deterministically.
// TODO: complete full leader schedule changes
#[derive(Clone)]
pub(crate) struct LeaderSchedule {
    context: Arc<Context>,
}

#[allow(unused)]
impl LeaderSchedule {
    pub fn new(context: Arc<Context>) -> Self {
        Self { context }
    }

    pub fn elect_leader(&self, round: u32, leader_offset: u32) -> AuthorityIndex {
        cfg_if::cfg_if! {
            // TODO: we need to differentiate the leader strategy in tests, so for
            // some type of testing (ex sim tests) we can use the staked approach.
            if #[cfg(test)] {
                AuthorityIndex::new_for_test((round + leader_offset) % self.context.committee.size() as u32)
            } else {
                self.elect_leader_stake_based(round, leader_offset)
            }
        }
    }

    pub fn elect_leader_stake_based(&self, round: u32, offset: u32) -> AuthorityIndex {
        assert!((offset as usize) < self.context.committee.size());

        // To ensure that we elect different leaders for the same round (using
        // different offset) we are using the round number as seed to shuffle in
        // a weighted way the results, but skip based on the offset.
        // TODO: use a cache in case this proves to be computationally expensive
        let mut seed_bytes = [0u8; 32];
        seed_bytes[32 - 4..].copy_from_slice(&(round).to_le_bytes());
        let mut rng = StdRng::from_seed(seed_bytes);

        let choices = self
            .context
            .committee
            .authorities()
            .map(|(index, authority)| (index, authority.stake as f32))
            .collect::<Vec<_>>();

        let leader_index = *choices
            .choose_multiple_weighted(&mut rng, self.context.committee.size(), |item| item.1)
            .expect("Weighted choice error: stake values incorrect!")
            .skip(offset as usize)
            .map(|(index, _)| index)
            .next()
            .unwrap();

        leader_index
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::{local_committee_and_keys, Parameters};
    use sui_protocol_config::ProtocolConfig;

    use super::*;
    use crate::metrics::test_metrics;

    #[test]
    fn test_elect_leader() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
        let leader_schedule = LeaderSchedule::new(context);

        assert_eq!(
            leader_schedule.elect_leader(0, 0),
            AuthorityIndex::new_for_test(0)
        );
        assert_eq!(
            leader_schedule.elect_leader(1, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader(5, 0),
            AuthorityIndex::new_for_test(1)
        );
        // ensure we elect different leaders for the same round for the multi-leader case
        assert_ne!(
            leader_schedule.elect_leader_stake_based(1, 1),
            leader_schedule.elect_leader_stake_based(1, 2)
        );
    }

    #[test]
    fn test_elect_leader_stake_based() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
        let leader_schedule = LeaderSchedule::new(context);

        assert_eq!(
            leader_schedule.elect_leader_stake_based(0, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader_stake_based(1, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader_stake_based(5, 0),
            AuthorityIndex::new_for_test(3)
        );
        // ensure we elect different leaders for the same round for the multi-leader case
        assert_ne!(
            leader_schedule.elect_leader_stake_based(1, 1),
            leader_schedule.elect_leader_stake_based(1, 2)
        );
    }
}
