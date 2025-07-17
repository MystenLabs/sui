// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, sync::Arc};

use tokio::time::Instant;

use crate::{
    block::{BlockRef, Round},
    context::Context,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

pub(crate) struct ThresholdClock {
    context: Arc<Context>,
    aggregator: StakeAggregator<QuorumThreshold>,
    round: Round,
    // Timestamp when the last quorum was form and the current round started.
    quorum_ts: Instant,
}

impl ThresholdClock {
    pub(crate) fn new(round: Round, context: Arc<Context>) -> Self {
        Self {
            context,
            aggregator: StakeAggregator::new(),
            round,
            quorum_ts: Instant::now(),
        }
    }

    /// Adds the block reference that have been accepted and advance the round accordingly.
    /// Returns true when the round has advanced.
    pub(crate) fn add_block(&mut self, block: BlockRef) -> bool {
        match block.round.cmp(&self.round) {
            // Blocks with round less then what we currently build are irrelevant here
            Ordering::Less => {}
            // If we processed block for round r, we also have stored 2f+1 blocks from r-1
            Ordering::Greater => {
                self.aggregator.clear();
                self.aggregator.add(block.author, &self.context.committee);
                self.round = block.round;
                self.quorum_ts = Instant::now();
            }
            Ordering::Equal => {
                let now = Instant::now();
                if self.aggregator.add(block.author, &self.context.committee) {
                    self.aggregator.clear();
                    // We have seen 2f+1 blocks for current round, advance
                    self.round = block.round + 1;

                    // Record the time of last quorum and new round start.
                    self.quorum_ts = now;

                    return true;
                }
                // Record delay from the start of the round.
                let hostname = &self.context.committee.authority(block.author).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .block_receive_delay
                    .with_label_values(&[hostname])
                    .inc_by(now.duration_since(self.quorum_ts).as_millis() as u64);
            }
        }

        false
    }

    /// Add the block references that have been successfully processed and advance the round accordingly. If the round
    /// has indeed advanced then the new round is returned, otherwise None is returned.
    #[cfg(test)]
    fn add_blocks(&mut self, blocks: Vec<BlockRef>) -> Option<Round> {
        let previous_round = self.round;
        for block_ref in blocks {
            self.add_block(block_ref);
        }
        (self.round > previous_round).then_some(self.round)
    }

    pub(crate) fn get_round(&self) -> Round {
        self.round
    }

    pub(crate) fn get_quorum_ts(&self) -> Instant {
        self.quorum_ts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockDigest;
    use consensus_config::AuthorityIndex;

    #[tokio::test]
    async fn test_threshold_clock_add_block() {
        let context = Arc::new(Context::new_for_test(4).0);
        let mut aggregator = ThresholdClock::new(0, context);

        aggregator.add_block(BlockRef::new(
            0,
            AuthorityIndex::new_for_test(0),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 0);
        aggregator.add_block(BlockRef::new(
            0,
            AuthorityIndex::new_for_test(1),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 0);
        aggregator.add_block(BlockRef::new(
            0,
            AuthorityIndex::new_for_test(2),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(0),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(3),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(BlockRef::new(
            2,
            AuthorityIndex::new_for_test(1),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(1),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(BlockRef::new(
            5,
            AuthorityIndex::new_for_test(2),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 5);
    }

    #[tokio::test]
    async fn test_threshold_clock_add_blocks() {
        let context = Arc::new(Context::new_for_test(4).0);
        let mut aggregator = ThresholdClock::new(0, context);

        let block_refs = vec![
            BlockRef::new(0, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            BlockRef::new(0, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            BlockRef::new(0, AuthorityIndex::new_for_test(2), BlockDigest::default()),
            BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            BlockRef::new(1, AuthorityIndex::new_for_test(3), BlockDigest::default()),
            BlockRef::new(2, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            BlockRef::new(1, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            BlockRef::new(5, AuthorityIndex::new_for_test(2), BlockDigest::default()),
        ];

        let result = aggregator.add_blocks(block_refs);
        assert_eq!(Some(5), result);
    }
}
