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
    aggregator: StakeAggregator<QuorumThreshold>,
    round: Round,
    quorum_ts: Instant,
    // Records the first time the leaders of round - 1 exist for proposal.
    proposal_leaders_ts: Option<Instant>,
    context: Arc<Context>,
}

impl ThresholdClock {
    pub(crate) fn new(round: Round, context: Arc<Context>) -> Self {
        Self {
            aggregator: StakeAggregator::new(),
            round,
            quorum_ts: Instant::now(),
            proposal_leaders_ts: None,
            context,
        }
    }

    /// Add the block references that have been successfully processed and advance the round accordingly. If the round
    /// has indeed advanced then the new round is returned, otherwise None is returned.
    pub(crate) fn add_blocks(
        &mut self,
        blocks: Vec<BlockRef>,
        proposal_leaders_exist: bool,
    ) -> Option<Round> {
        let previous_round = self.round;
        for block_ref in blocks {
            self.add_block(block_ref, proposal_leaders_exist);
        }
        (self.round > previous_round).then_some(self.round)
    }

    pub(crate) fn get_round(&self) -> Round {
        self.round
    }

    pub(crate) fn get_quorum_ts(&self) -> Instant {
        self.quorum_ts
    }

    pub(crate) fn get_proposal_leaders_ts(&self) -> Option<Instant> {
        self.proposal_leaders_ts
    }

    fn add_block(&mut self, block: BlockRef, proposal_leaders_exist: bool) {
        match block.round.cmp(&self.round) {
            // Blocks with round less then what we currently build are irrelevant here
            Ordering::Less => {}
            // If we processed block for round r, we also have stored 2f+1 blocks from r-1
            Ordering::Greater => {
                self.aggregator.clear();
                self.aggregator.add(block.author, &self.context.committee);
                if proposal_leaders_exist {
                    self.proposal_leaders_ts = Some(Instant::now());
                } else {
                    self.proposal_leaders_ts = None;
                }
                self.round = block.round;
            }
            Ordering::Equal => {
                if proposal_leaders_exist && self.proposal_leaders_ts.is_none() {
                    self.proposal_leaders_ts = Some(Instant::now());
                }
                if self.aggregator.add(block.author, &self.context.committee) {
                    self.aggregator.clear();
                    self.proposal_leaders_ts = None;
                    // We have seen 2f+1 blocks for current round, advance
                    self.round = block.round + 1;

                    // now record the time of receipt from last quorum
                    let now = Instant::now();
                    self.context
                        .metrics
                        .node_metrics
                        .quorum_receive_latency
                        .observe(now.duration_since(self.quorum_ts).as_secs_f64());
                    self.quorum_ts = now;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use consensus_config::AuthorityIndex;
    use tokio::time::sleep;

    use super::*;
    use crate::block::BlockDigest;

    #[tokio::test]
    async fn test_threshold_clock_add_block() {
        let context = Arc::new(Context::new_for_test(4).0);
        let mut aggregator = ThresholdClock::new(0, context);

        aggregator.add_block(
            BlockRef::new(0, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            true,
        );
        let leader_ts_round_0 = aggregator
            .get_proposal_leaders_ts()
            .expect("Leader ts should be set");
        assert_eq!(aggregator.get_round(), 0);
        aggregator.add_block(
            BlockRef::new(0, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            true,
        );
        assert_eq!(aggregator.get_round(), 0);
        assert_eq!(
            leader_ts_round_0,
            aggregator
                .get_proposal_leaders_ts()
                .expect("Leader ts should be set")
        );
        aggregator.add_block(
            BlockRef::new(0, AuthorityIndex::new_for_test(2), BlockDigest::default()),
            true,
        );
        assert!(aggregator.get_proposal_leaders_ts().is_none());
        assert_eq!(aggregator.get_round(), 1);
        sleep(Duration::from_millis(10)).await;
        aggregator.add_block(
            BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            true,
        );
        let leader_ts_round_1 = aggregator
            .get_proposal_leaders_ts()
            .expect("Leader ts should be set");
        assert!(leader_ts_round_1 > leader_ts_round_0);
        assert_eq!(aggregator.get_round(), 1);
        aggregator.add_block(
            BlockRef::new(1, AuthorityIndex::new_for_test(3), BlockDigest::default()),
            true,
        );
        assert_eq!(
            leader_ts_round_1,
            aggregator
                .get_proposal_leaders_ts()
                .expect("Leader ts should be set")
        );
        assert_eq!(aggregator.get_round(), 1);
        sleep(Duration::from_millis(10)).await;
        aggregator.add_block(
            BlockRef::new(2, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            true,
        );
        let leader_ts_round_2 = aggregator
            .get_proposal_leaders_ts()
            .expect("Leader ts should be set");
        assert!(leader_ts_round_2 > leader_ts_round_1);
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(
            BlockRef::new(1, AuthorityIndex::new_for_test(1), BlockDigest::default()),
            true,
        );
        assert_eq!(aggregator.get_round(), 2);
        aggregator.add_block(
            BlockRef::new(5, AuthorityIndex::new_for_test(2), BlockDigest::default()),
            false,
        );
        assert!(aggregator.get_proposal_leaders_ts().is_none());
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
            BlockRef::new(4, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            BlockRef::new(5, AuthorityIndex::new_for_test(2), BlockDigest::default()),
        ];

        let result = aggregator.add_blocks(block_refs, true);
        assert!(aggregator.get_proposal_leaders_ts().is_some());
        assert_eq!(Some(5), result);
    }
}
