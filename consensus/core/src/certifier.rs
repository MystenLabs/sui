// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use consensus_config::Committee;

use crate::{
    block::GENESIS_ROUND,
    context::Context,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    BlockAPI as _, BlockRef, CertifiedBlock, TransactionIndex, VerifiedBlock,
};

struct CertState {
    context: Arc<Context>,

    // Blocks received by this authority and votes on those blocks.
    blocks: BTreeMap<BlockRef, BlockInfo>,

    // Certified blocks pending to be processed outside of consensus.
    pending_certified_blocks: Vec<CertifiedBlock>,
}

impl CertState {
    fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            blocks: BTreeMap::new(),
            pending_certified_blocks: Vec::new(),
        }
    }

    fn add_voted_blocks(&mut self, _voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>) {}

    /// Returns the pending certified blocks.
    fn take_certified_blocks(&mut self) -> Vec<CertifiedBlock> {
        self.pending_certified_blocks.drain(..).collect()
    }

    // Updates votes for certification of transactions and blocks in the causal history of the voter block.
    // Returns newly certified blocks. A certified block must meet these criteria within its voting round:
    // - A quorum of authorities link to the block via ancestors.
    // - Every transaction in the block is either accepted or rejected by a quorum.
    // TODO(fastpath): add randomized tests.
    fn update_certification_votes(&mut self, voter_block: &VerifiedBlock) -> Vec<CertifiedBlock> {
        let mut certified_blocks = vec![];

        // When a block has an explicit vote, record the rejections. The rest of the transactions are implicitly accepted.
        // NOTE: it is very important to count the rejection votes before checking for certification of a block.
        // Otherwise a transaction that should not be certified or should be rejected may become accepted along with its block.
        for block_votes in voter_block.transaction_votes() {
            let Some(block_info) = self.blocks.get_mut(&block_votes.block_ref) else {
                // TODO(fastpath): ensure the voted block exists in the DAG, with BlockManager.
                // If the block is not found, it is outside of GC bound.
                continue;
            };
            for reject in &block_votes.rejects {
                // TODO(fastpath): validate votes after ensuring existence of voted block in BlockManager.
                block_info
                    .reject_votes
                    .entry(*reject)
                    .or_default()
                    .add_unique(voter_block.author(), &self.context.committee);
            }
        }

        // Implicitly accept transactions in the DAG of the block. This is the common case.
        // And check for certification of the block after the voting.
        //
        // NOTE: if the voter or an ancestor authority equivocates, it is possible an explicitly voted
        // block in transaction_votes is not in the DAG of any voter block ancestor refs. So blocks in
        // transaction_votes and their authority ancestors get another chance to receive implicit
        // accept votes.
        for ancestor in voter_block
            .ancestors()
            .iter()
            .chain(voter_block.transaction_votes().iter().map(|b| &b.block_ref))
        {
            let blocks = self.update_accept_votes_for_ancestor_authority(voter_block, *ancestor);
            certified_blocks.extend(blocks);
        }

        certified_blocks
    }

    // This function updates the implicit accept votes for the target block and its ancestor blocks
    // of the same authority.
    //
    // All blocks in the causal history of the voter / target blocks can in theory receive implicit
    // accept votes. But traversing the causal history of voter / target blocks are very expensive.
    // Instead, only voter blocks's ancestors are traversed in update_certification_votes().
    // And only ancestors from the same authority are traversed here. This will not skip implicitly
    // voting accept on any block, when neither the voter or target authority is equivocating.
    //
    // If any one of the voter or target authority is equivocating, there can be blocks in the causal
    // history of the voter / target blocks that do not receive implicit accept votes.
    // This is ok though. All blocks are still finalized with the fastpath commit rule, which counts
    // votes differently. There is no safety or liveness issue with not voting accept on some blocks.
    // The only downside is that these blocks may not get certified and sent to the fastpath.
    fn update_accept_votes_for_ancestor_authority(
        &mut self,
        voter_block: &VerifiedBlock,
        mut target: BlockRef,
    ) -> Vec<CertifiedBlock> {
        let mut certified_blocks = vec![];
        while target.round > GENESIS_ROUND
            && target.round
                >= voter_block.round().saturating_sub(
                    self.context
                        .protocol_config
                        .consensus_voting_rounds_as_option()
                        .unwrap(),
                )
        {
            let Some(target_block_info) = self.blocks.get_mut(&target) else {
                // The target block has been GC'ed.
                break;
            };
            if !target_block_info
                .accept_votes
                .add_unique(voter_block.author(), &self.context.committee)
            {
                // Stop voting because this target block and its ancestors from the same authority have been voted.
                break;
            }
            if let Some(b) = target_block_info.take_certified_output(&self.context.committee) {
                certified_blocks.push(b);
            }
            // Try voting on the ancestor of the same authority.
            // This should stop at the first ancestor, on blocks verified by BlockVerifier.
            // TODO: fix ancestors order in tests.
            let Some(ancestor) = target_block_info
                .block
                .as_ref()
                .and_then(|b| b.ancestors().first())
            else {
                cfg_if::cfg_if! {
                    if #[cfg(test)] {
                        // This is expected in tests where blocks are created without
                        // proper ancestor order.
                        break;
                    } else {
                        panic!("Block has no ancestor: {:?}", target_block_info.block,);
                    }
                }
            };
            if target.author != ancestor.author {
                cfg_if::cfg_if! {
                    if #[cfg(test)] {
                        // This is expected in tests where blocks are created without
                        // proper ancestor order.
                        break;
                    } else {
                        panic!(
                            "1st ancestor is not from the same authority: {:?}",
                            target_block_info.block,
                        );
                    }
                }
            }
            target = *ancestor;
        }
        certified_blocks
    }
}

pub(crate) struct Certifier {
    context: Arc<Context>,
}

struct BlockInfo {
    // Content of the block.
    // None if the blocks has not been received.
    block: Option<VerifiedBlock>,
    // Rejection votes on this blocks by this authority.
    // None if the block has not been received.
    own_reject_votes: Option<Vec<TransactionIndex>>,
    // Accumulates implicit accept votes for all transactions.
    accept_votes: StakeAggregator<QuorumThreshold>,
    // Accumulates reject votes per transaction.
    reject_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    // Whether this block has been sent to output after it has been certified.
    certified_output: bool,
}

impl BlockInfo {
    fn new(block: VerifiedBlock, reject_votes: Vec<TransactionIndex>) -> Self {
        Self {
            block: Some(block),
            own_reject_votes: Some(reject_votes),
            accept_votes: StakeAggregator::new(),
            reject_votes: BTreeMap::new(),
            certified_output: false,
        }
    }

    fn new_empty() -> Self {
        Self {
            block: None,
            own_reject_votes: None,
            accept_votes: StakeAggregator::new(),
            reject_votes: BTreeMap::new(),
            certified_output: false,
        }
    }

    // If this block has been certified but has not been sent to output, returns the output.
    // Otherwise, returns None.
    fn take_certified_output(&mut self, committee: &Committee) -> Option<CertifiedBlock> {
        let Some(block) = self.block.as_ref() else {
            return None;
        };
        if self.certified_output {
            return None;
        }
        if !self.accept_votes.reached_threshold(committee) {
            return None;
        }
        let mut rejected = vec![];
        for (idx, reject_votes) in &self.reject_votes {
            // The transaction is certified to be rejected.
            if reject_votes.reached_threshold(committee) {
                rejected.push(*idx);
                continue;
            }
            // If the transaction is not certified to be rejected or accepted, the block cannot
            // be considered as certified.
            if self
                .accept_votes
                .stake()
                .checked_sub(reject_votes.stake())
                .unwrap()
                < committee.quorum_threshold()
            {
                return None;
            }
        }
        self.certified_output = true;
        Some(CertifiedBlock {
            block: block.clone(),
            rejected,
        })
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeSet;

    use consensus_config::AuthorityIndex;

    use crate::{
        block::BlockTransactionVotes, context::Context, test_dag_builder::DagBuilder, TestBlock,
    };

    use super::*;

    #[tokio::test]
    async fn test_voting_basic() {
        telemetry_subscribers::init_for_testing();
        let num_authorities: u32 = 7;
        let (context, _) = Context::new_for_test(num_authorities as usize);
        let context = Arc::new(context);
        let mut cert_state = CertState::new(context.clone());

        // Create minimal connected blocks up to round voting_rounds - 1,
        // and add a final round with full blocks connections.
        let voting_rounds = context.protocol_config.consensus_voting_rounds();
        let num_rounds = voting_rounds - 1;
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=num_rounds)
            .min_ancestor_links(false, None);
        dag_builder.layer(voting_rounds).build();

        // Add all created blocks to DagState.
        let mut all_blocks: Vec<_> = dag_builder.all_blocks();
        all_blocks.sort_by_key(|b| b.reference());
        cert_state.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());

        let certified_blocks = cert_state.take_certified_blocks();

        // It is expected that all blocks with round < voting_rounds are certified.
        let voted_block_refs = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() < voting_rounds {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        let certified_block_refs = certified_blocks
            .iter()
            .map(|b| b.block.reference())
            .collect::<BTreeSet<_>>();

        let diff = voted_block_refs
            .difference(&certified_block_refs)
            .collect::<Vec<_>>();
        assert!(diff.is_empty(), "Blocks {:?} are not certified", diff);

        let diff = certified_block_refs
            .difference(&voted_block_refs)
            .collect::<Vec<_>>();
        assert!(
            diff.is_empty(),
            "Certified blocks {:?} are unexpected",
            diff
        );

        // Ensure no transaction is rejected.
        for b in &certified_blocks {
            assert!(b.rejected.is_empty());
        }
    }

    #[tokio::test]
    async fn test_voting_with_rejections() {
        telemetry_subscribers::init_for_testing();
        let num_authorities: u32 = 4;
        let (context, _) = Context::new_for_test(num_authorities as usize);
        let context = Arc::new(context);
        let mut cert_state = CertState::new(context.clone());

        // Create connected blocks up to voting_rounds, with only 3 authorities.
        let voting_rounds = context.protocol_config.consensus_voting_rounds();
        let last_round = voting_rounds + 1;
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=last_round)
            .block_authorities((0..3).map(AuthorityIndex::new_for_test).collect())
            .include_transactions(4)
            .build();

        let mut all_blocks: Vec<_> = dag_builder.all_blocks();
        all_blocks.sort_by_key(|b| b.reference());

        let last_block = all_blocks.last().unwrap().clone();
        assert_eq!(last_block.round(), last_round);

        let mut next_ancestors = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() == last_round {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Create a block outside of voting rounds, which should not be accepted.
        let out_of_range_block = VerifiedBlock::new_for_test(TestBlock::new(1, 3).build());
        next_ancestors.push(out_of_range_block.reference());
        all_blocks.push(out_of_range_block.clone());

        // Create a block not voted by any other block.
        let mut ignored_block_ancestors = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() == last_round - 1 {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ignored_block_ancestors.push(out_of_range_block.reference());
        let ignored_block = VerifiedBlock::new_for_test(
            TestBlock::new(last_round, 3)
                .set_ancestors(ignored_block_ancestors)
                .build(),
        );
        all_blocks.push(ignored_block);

        // Create blocks rejecting transaction 2 in last_block, linking to out_of_range_block where no vote should be counted,
        // and accepting other blocks and transactions.
        let final_round_blocks: Vec<_> = (0..4)
            .map(|i| {
                let test_block = TestBlock::new(last_round + 1, i)
                    .set_transaction_votes(vec![BlockTransactionVotes {
                        block_ref: last_block.reference(),
                        rejects: vec![2],
                    }])
                    .set_ancestors(next_ancestors.clone())
                    .build();
                VerifiedBlock::new_for_test(test_block)
            })
            .collect();
        all_blocks.extend(final_round_blocks);

        // Accept all created blocks.
        cert_state.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());

        let certified_blocks = cert_state.take_certified_blocks();

        // It is expected that all blocks with round <= last_round and from authorities [0,1,2] are certified.
        // The rest of blocks are not.
        let voted_block_refs = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() <= last_round && b.author() != AuthorityIndex::new_for_test(3) {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        let certified_block_refs = certified_blocks
            .iter()
            .map(|b| b.block.reference())
            .collect::<BTreeSet<_>>();

        let diff = voted_block_refs
            .difference(&certified_block_refs)
            .collect::<Vec<_>>();
        assert!(diff.is_empty(), "Blocks {:?} are not certified", diff);

        let diff = certified_block_refs
            .difference(&voted_block_refs)
            .collect::<Vec<_>>();
        assert!(
            diff.is_empty(),
            "Certified blocks {:?} are unexpected",
            diff
        );

        // Ensure only the expected transaction is rejected.
        for b in &certified_blocks {
            if b.block.reference() != last_block.reference() {
                assert!(b.rejected.is_empty());
                continue;
            }
            assert_eq!(b.rejected, vec![2]);
        }
    }
}
