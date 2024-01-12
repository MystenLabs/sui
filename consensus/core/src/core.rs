// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::{
    block::{Block, BlockAPI, BlockRef, BlockV1, Round},
    context::Context,
    threshold_clock::ThresholdClock,
};

use mysten_metrics::monitored_scope;

#[allow(dead_code)]
pub(crate) struct Core {
    context: Arc<Context>,
    threshold_clock: ThresholdClock,
    last_own_block: Block,
}

#[allow(dead_code)]
impl Core {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        // TODO: restore the threshold clock round based on the last quorum data in storage when crash/recover
        let threshold_clock = ThresholdClock::new(0, context.clone());

        Self {
            context,
            threshold_clock,
            last_own_block: Block::V1(BlockV1::default()), // TODO: restore on crash/recovery
        }
    }

    /// Processes the provided blocks and accepts them if possible when their causal history exists.
    /// The method returns the references of parents that are unknown and need to be fetched.
    pub(crate) fn add_blocks(&mut self, _blocks: Vec<Block>) -> Vec<BlockRef> {
        let _scope = monitored_scope("Core::add_blocks");

        vec![]
    }

    /// Force creating a new block for the dictated round. This is used when a leader timeout occurs.
    pub fn force_new_block(&mut self, round: Round) -> Option<Block> {
        if self.last_proposed_round() < round {
            self.context.metrics.node_metrics.leader_timeout_total.inc();
            self.try_new_block(true)
        } else {
            None
        }
    }

    /// Attempts to propose a new block for the next round. If a block has already proposed for latest
    /// or earlier round, then no block is created and None is returned.
    pub(crate) fn try_new_block(&mut self, force_new_block: bool) -> Option<Block> {
        let _scope = monitored_scope("Core::try_new_block");

        let clock_round = self.threshold_clock.get_round();
        if clock_round <= self.last_proposed_round() {
            return None;
        }

        // create a new block either because we want to "forcefully" propose a block due to a leader timeout,
        // or because we are actually ready to produce the block (leader exists)
        if force_new_block || self.ready_new_block() {
            // TODO: produce the block for the clock_round
        }

        None
    }

    fn ready_new_block(&self) -> bool {
        // TODO: check that we are ready to produce a new block. This will mainly check that the leader of the previous
        // quorum exists.
        true
    }

    fn last_proposed_round(&self) -> Round {
        self.last_own_block.round()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::metrics::test_metrics;
    use consensus_config::Committee;
    use consensus_config::{AuthorityIndex, Parameters};
    use sui_protocol_config::ProtocolConfig;

    #[test]
    fn test_core() {
        let (committee, _) = Committee::new_for_test(0, vec![1, 1, 1, 1]);
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));

        let core = Core::new(context);

        assert_eq!(core.last_proposed_round(), 0);
    }
}
