// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, sync::Arc, time::Instant};

use crate::{
    block::{BlockRef, Round},
    context::Context,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

#[allow(unused)]

pub(crate) struct ThresholdClock {
    aggregator: StakeAggregator<QuorumThreshold>,
    round: Round,
    last_quorum_ts: Instant,
    context: Arc<Context>,
}

#[allow(unused)]

impl ThresholdClock {
    pub(crate) fn new(round: Round, context: Arc<Context>) -> Self {
        Self {
            aggregator: StakeAggregator::new(),
            round,
            last_quorum_ts: Instant::now(),
            context,
        }
    }

    pub fn last_quorum_ts(&self) -> Instant {
        self.last_quorum_ts
    }

    pub fn add_block(&mut self, block: BlockRef) {
        match block.round.cmp(&self.round) {
            // Blocks with round less then what we currently build are irrelevant here
            Ordering::Less => {}
            // If we processed block for round r, we also have stored 2f+1 blocks from r-1
            Ordering::Greater => {
                self.aggregator.clear();
                self.aggregator.add(block.author, &self.context.committee);
                self.round = block.round;
            }
            Ordering::Equal => {
                if self.aggregator.add(block.author, &self.context.committee) {
                    self.aggregator.clear();
                    // We have seen 2f+1 blocks for current round, advance
                    self.round = block.round + 1;

                    // now record the time of receipt from last quorum
                    let now = Instant::now();
                    self.context
                        .metrics
                        .node_metrics
                        .quorum_receive_latency
                        .observe(now.duration_since(self.last_quorum_ts).as_secs_f64());
                    self.last_quorum_ts = now;
                }
            }
        }
    }

    pub fn get_round(&self) -> Round {
        self.round
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockDigest;
    use crate::metrics::test_metrics;
    use consensus_config::Committee;
    use consensus_config::{AuthorityIndex, Parameters};
    use sui_protocol_config::ProtocolConfig;

    #[test]
    fn test_threshold_clock() {
        let committee = Committee::new_for_test(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
        let mut aggregator = ThresholdClock::new(0, context);

        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(0),
            0,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 0);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(1),
            0,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 0);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(2),
            0,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(0),
            1,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(3),
            1,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(1),
            2,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(1),
            1,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(BlockRef::new_test(
            AuthorityIndex::new_for_test(2),
            5,
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 5);
    }
}
