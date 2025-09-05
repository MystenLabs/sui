// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, sync::Arc};

use consensus_types::block::{BlockRef, Round};
use tokio::time::Instant;
use tracing::{debug, info};

use crate::{
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
        info!("Recovered ThresholdClock at round {}", round);
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
            Ordering::Less => false,
            Ordering::Equal => {
                let now = Instant::now();
                if self.aggregator.add(block.author, &self.context.committee) {
                    self.aggregator.clear();
                    // We have seen 2f+1 blocks for current round, advance
                    self.round = block.round + 1;
                    // Record the time of last quorum and new round start.
                    self.quorum_ts = now;
                    debug!(
                        "ThresholdClock advanced to round {} with block {} completing quorum",
                        self.round, block
                    );
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
                false
            }
            // If we processed block for round r, we also have stored 2f+1 blocks from r-1
            Ordering::Greater => {
                self.aggregator.clear();
                if self.aggregator.add(block.author, &self.context.committee) {
                    // Even though this is the first block of the round, there is still a quorum at block.round.
                    self.round = block.round + 1;
                } else {
                    // There is a quorum at block.round - 1 but not block.round.
                    self.round = block.round;
                };
                self.quorum_ts = Instant::now();
                debug!(
                    "ThresholdClock advanced to round {} with block {} catching up round",
                    self.round, block
                );
                true
            }
        }
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
    use consensus_types::block::BlockDigest;

    use super::*;
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

    #[tokio::test]
    async fn test_threshold_clock_add_block_min_committee() {
        let context = Arc::new(Context::new_for_test(1).0);
        let mut aggregator = ThresholdClock::new(10, context);

        // Adding a past block should not advance the round.
        assert!(!aggregator.add_block(BlockRef::new(
            9,
            AuthorityIndex::new_for_test(0),
            BlockDigest::default(),
        )));
        assert_eq!(aggregator.get_round(), 10);

        // Adding one block is enough to complete the quorum.
        assert!(aggregator.add_block(BlockRef::new(
            10,
            AuthorityIndex::new_for_test(0),
            BlockDigest::default(),
        )));
        assert_eq!(aggregator.get_round(), 11);

        // Adding a catch up block should both advance the round and complete the quorum.
        aggregator.add_block(BlockRef::new(
            20,
            AuthorityIndex::new_for_test(0),
            BlockDigest::default(),
        ));
        assert_eq!(aggregator.get_round(), 21);
    }
}
