// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::{base_types::ObjectID, messages_consensus::Round};

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeferralKey {
    // For transactions deferred until new randomness is available (whether delayd due to
    // DKG, or skipped commits).
    Randomness {
        deferred_from_round: Round, // commit round, not randomness round
    },
    // ConsensusRound deferral key requires both the round to which the tx should be deferred (so that
    // we can efficiently load all txns that are now ready), and the round from which it has been
    // deferred (so that multiple rounds can efficiently defer to the same future round).
    ConsensusRound {
        future_round: Round,
        deferred_from_round: Round,
    },
}

impl DeferralKey {
    pub fn new_for_randomness(deferred_from_round: Round) -> Self {
        Self::Randomness {
            deferred_from_round,
        }
    }

    pub fn new_for_consensus_round(future_round: Round, deferred_from_round: Round) -> Self {
        Self::ConsensusRound {
            future_round,
            deferred_from_round,
        }
    }

    pub fn full_range_for_randomness() -> (Self, Self) {
        (
            Self::Randomness {
                deferred_from_round: 0,
            },
            Self::Randomness {
                deferred_from_round: u64::MAX,
            },
        )
    }

    // Returns a range of deferral keys that are deferred up to the given consensus round.
    pub fn range_for_up_to_consensus_round(consensus_round: Round) -> (Self, Self) {
        (
            Self::ConsensusRound {
                future_round: 0,
                deferred_from_round: 0,
            },
            Self::ConsensusRound {
                future_round: consensus_round.checked_add(1).unwrap(),
                deferred_from_round: 0,
            },
        )
    }

    pub fn deferred_from_round(&self) -> Round {
        match self {
            Self::Randomness {
                deferred_from_round,
            } => *deferred_from_round,
            Self::ConsensusRound {
                deferred_from_round,
                ..
            } => *deferred_from_round,
        }
    }
}

#[derive(Debug)]
pub enum DeferralReason {
    RandomnessNotReady,

    // The list of objects are congested objects.
    SharedObjectCongestion(Vec<ObjectID>),
}

pub fn transaction_deferral_within_limit(
    deferral_key: &DeferralKey,
    max_deferral_rounds_for_congestion_control: u64,
) -> bool {
    if let DeferralKey::ConsensusRound {
        future_round,
        deferred_from_round,
    } = deferral_key
    {
        return (future_round - deferred_from_round) <= max_deferral_rounds_for_congestion_control;
    }

    // TODO: drop transactions at the end of the queue if the queue is too long.

    true
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;
    use typed_store::DBMapUtils;
    use typed_store::Map;
    use typed_store::{
        rocks::{DBMap, MetricConf},
        traits::{TableSummary, TypedStoreDebug},
    };

    #[tokio::test]
    async fn test_deferral_key_sort_order() {
        use rand::prelude::*;

        #[derive(DBMapUtils)]
        struct TestDB {
            deferred_certs: DBMap<DeferralKey, ()>,
        }

        // get a tempdir
        let tempdir = tempfile::tempdir().unwrap();

        let db = TestDB::open_tables_read_write(
            tempdir.path().to_owned(),
            MetricConf::new("test_db"),
            None,
            None,
        );

        for _ in 0..10000 {
            let future_round = rand::thread_rng().gen_range(0..u64::MAX);
            let current_round = rand::thread_rng().gen_range(0..u64::MAX);

            let key = DeferralKey::new_for_consensus_round(future_round, current_round);
            db.deferred_certs.insert(&key, &()).unwrap();
        }

        let mut previous_future_round = 0;
        for (key, _) in db.deferred_certs.unbounded_iter() {
            match key {
                DeferralKey::Randomness { .. } => (),
                DeferralKey::ConsensusRound { future_round, .. } => {
                    assert!(previous_future_round <= future_round);
                    previous_future_round = future_round;
                }
            }
        }
    }

    // Tests that fetching deferred transactions up to a given consensus rounds works as expected.
    #[tokio::test]
    async fn test_fetching_deferred_txs() {
        use rand::prelude::*;

        #[derive(DBMapUtils)]
        struct TestDB {
            deferred_certs: DBMap<DeferralKey, ()>,
        }

        // get a tempdir
        let tempdir = tempfile::tempdir().unwrap();

        let db = TestDB::open_tables_read_write(
            tempdir.path().to_owned(),
            MetricConf::new("test_db"),
            None,
            None,
        );

        // All future rounds are between 100 and 300.
        let min_future_round = 100;
        let max_future_round = 300;
        for _ in 0..10000 {
            let future_round = rand::thread_rng().gen_range(min_future_round..=max_future_round);
            let current_round = rand::thread_rng().gen_range(0..u64::MAX);

            db.deferred_certs
                .insert(
                    &DeferralKey::new_for_consensus_round(future_round, current_round),
                    &(),
                )
                .unwrap();
            // Add a randomness deferral txn to make sure that it won't show up when fetching deferred consensus round txs.
            db.deferred_certs
                .insert(&DeferralKey::new_for_randomness(current_round), &())
                .unwrap();
        }

        // Fetch all deferred transactions up to consensus round 200.
        let (min, max) = DeferralKey::range_for_up_to_consensus_round(200);
        let mut previous_future_round = 0;
        let mut result_count = 0;
        for result in db
            .deferred_certs
            .safe_iter_with_bounds(Some(min), Some(max))
        {
            let (key, _) = result.unwrap();
            match key {
                DeferralKey::Randomness { .. } => {
                    panic!("Should not receive randomness deferral txn.")
                }
                DeferralKey::ConsensusRound { future_round, .. } => {
                    assert!(previous_future_round <= future_round);
                    previous_future_round = future_round;
                    assert!(future_round <= 200);
                    result_count += 1;
                }
            }
        }
        assert!(result_count > 0);
    }
}
