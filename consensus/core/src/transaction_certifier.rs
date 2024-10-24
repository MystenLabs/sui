// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use consensus_config::Committee;
use mysten_metrics::monitored_mpsc::UnboundedSender;
use parking_lot::RwLock;

use crate::{
    block::GENESIS_ROUND,
    context::Context,
    dag_state::DagState,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    BlockAPI as _, BlockRef, CertifiedBlock, CertifiedBlocksOutput, Round, TransactionIndex,
    VerifiedBlock,
};

/// TransactionCertifier tries to certify transactions and send them to execute on the fastpath.
///
/// A transaction is certified if a quorum of authorities voted to accept it. A block is certified
/// if every transaction in the block is either certified or rejected. Output of TransactionCertifier
/// is batched by certified blocks. Once a certified block is sent to output, it is assumed external
/// clients can start receiving execution confirmations on fastpath transactions in the block.
///
/// The invariant is that if a quorum of authorities certified a transaction, then the
/// transaction must also be finalized via consensus commit eventually. The reverse is not
/// necessarily true though, because fastpath execution is only an optimistic latency optimization.
/// The implementation here takes advantage of this fact. By narrowing the scope of fastpath
/// execution, post-commit finalization can be made simpler, more efficient and more robust.
#[derive(Clone)]
pub(crate) struct TransactionCertifier {
    // The state of blocks being voted on and certified.
    certifier_state: Arc<RwLock<CertifierState>>,
    // The state of the DAG.
    dag_state: Arc<RwLock<DagState>>,
    // An unbounded channel to send certified blocks to the block handler
    // as part of the consensus output.
    certified_blocks_sender: UnboundedSender<CertifiedBlocksOutput>,
}

impl TransactionCertifier {
    pub(crate) fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        certified_blocks_sender: UnboundedSender<CertifiedBlocksOutput>,
    ) -> Self {
        Self {
            certifier_state: Arc::new(RwLock::new(CertifierState::new(context))),
            dag_state,
            certified_blocks_sender,
        }
    }

    /// Process own votes on input blocks and votes contained in the input blocks.
    /// Newly certified blocks are sent to the output channel.
    ///
    /// NOTE: blocks from commit sync do not need to be added, because transactions
    /// arrived via commit sync should not need to be executed on the fastpath.
    // TODO: add own proposed blocks, even though the contained votes have been processed.
    pub(crate) fn add_voted_blocks(
        &self,
        voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        let certified_blocks = self.certifier_state.write().add_voted_blocks(voted_blocks);
        if certified_blocks.is_empty() {
            return;
        }

        // Only send a block at round R to execute on fastpath if the next round to propose is <= R+2.
        // So if a quorum of authorities sends a block to fastpath and returns the effect digests to a client,
        // the quorum is also promising they can only propose at round <= R+2. Then every block at round R+3
        // must have a link to a certificate of the fastpath block.
        let next_propose_round = self.dag_state.read().next_propose_round();
        let certified_blocks: Vec<_> = certified_blocks
            .into_iter()
            .filter(|b| b.block.round() + 2 >= next_propose_round)
            .collect();
        if certified_blocks.is_empty() {
            return;
        }

        if let Err(e) = self.certified_blocks_sender.send(CertifiedBlocksOutput {
            blocks: certified_blocks,
        }) {
            tracing::warn!("Failed to send certified blocks: {:?}", e);
        }
    }

    /// Updates the GC round and cleans up obsolete internal state.
    // TODO: use GC to clean up CertifierState.
    #[allow(unused)]
    pub(crate) fn update_gc_round(&self, gc_round: Round) {
        self.certifier_state.write().update_gc_round(gc_round);
    }
}

/// CertifierState keeps track of votes received by each transaction and block,
/// and helps determine if votes reach a quorum. Votes can start accumulating even
/// before the target block is received by this authority.
struct CertifierState {
    context: Arc<Context>,

    // Blocks received by this authority and votes on those blocks.
    votes: BTreeMap<BlockRef, VoteInfo>,

    // Highest round where blocks are GC'ed.
    gc_round: Round,
}

impl CertifierState {
    fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            votes: BTreeMap::new(),
            gc_round: GENESIS_ROUND,
        }
    }

    fn add_voted_blocks(
        &mut self,
        voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) -> Vec<CertifiedBlock> {
        let mut certified_blocks = vec![];
        for (voted_block, reject_txn_votes) in voted_blocks {
            let blocks = self.add_voted_block(voted_block, reject_txn_votes);
            certified_blocks.extend(blocks);
        }
        certified_blocks
    }

    fn add_voted_block(
        &mut self,
        voted_block: VerifiedBlock,
        reject_txn_votes: Vec<TransactionIndex>,
    ) -> Vec<CertifiedBlock> {
        let mut certified_blocks = vec![];

        if voted_block.round() <= self.gc_round {
            // Block is outside of GC bound.
            return vec![];
        }

        let vote_info = self.votes.entry(voted_block.reference()).or_default();
        if vote_info.block.is_some() {
            // Input block has already been processed and added to the state.
            return vec![];
        }
        vote_info.block = Some(voted_block.clone());
        vote_info.own_reject_txn_votes = Some(reject_txn_votes.clone());

        // Add own reject txn votes for the block.
        for reject in &reject_txn_votes {
            vote_info
                .reject_txn_votes
                .entry(*reject)
                .or_default()
                .add_unique(self.context.own_index, &self.context.committee);
        }

        // Add own accept votes for the block.
        vote_info
            .accept_block_votes
            .add_unique(self.context.own_index, &self.context.committee);

        // Check if the input block is now certified after the updates above.
        // NOTE: votes can already exist for the block and its transactions.
        if let Some(certified_block) = vote_info.take_certified_output(&self.context.committee) {
            certified_blocks.push(certified_block);
        }

        // Update reject votes from the block first. Otherwise another block can get certified when it should not be.
        for block_votes in voted_block.transaction_votes() {
            if block_votes.block_ref.round <= self.gc_round {
                // Block is outside of GC bound.
                continue;
            }
            let vote_info = self.votes.entry(block_votes.block_ref).or_default();
            for reject in &block_votes.rejects {
                vote_info
                    .reject_txn_votes
                    .entry(*reject)
                    .or_default()
                    .add_unique(voted_block.author(), &self.context.committee);
            }
            if let Some(certified_block) = vote_info.take_certified_output(&self.context.committee)
            {
                certified_blocks.push(certified_block);
            }
        }

        // Update accept votes from the block after updating reject votes.
        // Only parent round blocks receive accept votes on the fastpath.
        for ancestor in voted_block.ancestors() {
            if ancestor.round + 1 != voted_block.round() || ancestor.round <= self.gc_round {
                // Ancestor is not a parent or outside of GC bound.
                continue;
            }
            let vote_info = self.votes.entry(*ancestor).or_default();
            vote_info
                .accept_block_votes
                .add_unique(voted_block.author(), &self.context.committee);
            if let Some(certified_block) = vote_info.take_certified_output(&self.context.committee)
            {
                certified_blocks.push(certified_block);
            }
        }

        certified_blocks
    }

    #[allow(unused)]
    fn update_gc_round(&mut self, gc_round: Round) {
        self.gc_round = gc_round;
        while let Some((block_ref, _)) = self.votes.first_key_value() {
            if block_ref.round <= self.gc_round {
                self.votes.pop_first();
            } else {
                break;
            }
        }
    }
}

/// VoteInfo keeps track of votes received for each transaction of this block,
/// possibly even before the block is received by this authority.
struct VoteInfo {
    // Content of the block.
    // None if the blocks has not been received.
    block: Option<VerifiedBlock>,
    // Rejection votes by this authority on this block.
    // None if the block has not been received.
    // The purpose is to help propagate votes from receiving a block to after the block is
    // in a consensus commit.
    own_reject_txn_votes: Option<Vec<TransactionIndex>>,
    // Accumulates implicit accept votes for all transactions.
    accept_block_votes: StakeAggregator<QuorumThreshold>,
    // Accumulates reject votes per transaction.
    reject_txn_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    // Whether this block has been certified already.
    is_certified: bool,
}

impl VoteInfo {
    fn new() -> Self {
        Self {
            block: None,
            own_reject_txn_votes: None,
            accept_block_votes: StakeAggregator::new(),
            reject_txn_votes: BTreeMap::new(),
            is_certified: false,
        }
    }

    // If this block can now be certified but has not been sent to output, returns the output.
    // Otherwise, returns None.
    fn take_certified_output(&mut self, committee: &Committee) -> Option<CertifiedBlock> {
        if self.is_certified {
            // Skip if already certified.
            return None;
        }
        let Some(block) = self.block.as_ref() else {
            // Skip if the block has not been received.
            return None;
        };
        if !self.accept_block_votes.reached_threshold(committee) {
            // Skip if the block is not certified.
            return None;
        }
        let mut rejected = vec![];
        for (idx, reject_txn_votes) in &self.reject_txn_votes {
            // The transaction is certified to be rejected.
            if reject_txn_votes.reached_threshold(committee) {
                rejected.push(*idx);
                continue;
            }
            // If a transaction does not have a quorum of accept votes minus the reject votes,
            // it is neither rejected nor certified. In this case the whole block cannot
            // be considered as certified.

            // accept_block_votes can be < reject_txn_votes on the transaction when reject_txn_votes
            // come from blocks more than 1 round higher, which do not add to the
            // accept votes of the block.
            //
            // Also, the accept votes used to certify a transactions is undercounted here.
            // If a block has accept votes from a quorum of authorities A, B and C, but one transaction
            // has a reject vote from D, the transaction and block are technically certified
            // and can be sent to fastpath. However, the computation here will not certify the transaction
            // or the block, which is fine because the fastpath certification is optimistic.
            // The definite status of the transaction will be decided during post commit finalization.
            if self
                .accept_block_votes
                .stake()
                .saturating_sub(reject_txn_votes.stake())
                < committee.quorum_threshold()
            {
                return None;
            }
        }
        // The block is certified.
        self.is_certified = true;
        Some(CertifiedBlock {
            block: block.clone(),
            rejected,
        })
    }
}

impl Default for VoteInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeSet;

    use consensus_config::AuthorityIndex;
    use itertools::Itertools;
    use rand::seq::SliceRandom as _;

    use crate::{
        block::BlockTransactionVotes, context::Context, test_dag_builder::DagBuilder, TestBlock,
        Transaction,
    };

    use super::*;

    #[tokio::test]
    async fn test_vote_info_basic() {
        let (context, _) = Context::new_for_test(7);
        let committee = &context.committee;

        // No accept votes.
        {
            let mut vote_info = VoteInfo::new();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            vote_info.own_reject_txn_votes = Some(vec![]);

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // Accept votes but not enough.
        {
            let mut vote_info = VoteInfo::new();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            vote_info.own_reject_txn_votes = Some(vec![]);
            for i in 0..4 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // Enough accept votes but no block.
        {
            let mut vote_info = VoteInfo::new();
            for i in 0..5 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // A quorum of accept votes and block exists.
        {
            let mut vote_info = VoteInfo::new();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            vote_info.own_reject_txn_votes = Some(vec![]);
            for i in 0..4 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }

            // The block is not certified.
            assert!(vote_info.take_certified_output(committee).is_none());

            // Add 1 more accept vote from a different authority.
            vote_info
                .accept_block_votes
                .add_unique(AuthorityIndex::new_for_test(4), committee);

            // The block is now certified.
            let certified_block = vote_info.take_certified_output(committee).unwrap();
            assert_eq!(certified_block.block.reference(), block.reference());

            // Certified block cannot be taken again.
            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // A quorum of accept and reject votes.
        {
            let mut vote_info = VoteInfo::new();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            vote_info.own_reject_txn_votes = Some(vec![]);
            // Add 5 accept votes which form a quorum.
            for i in 0..5 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }
            // For transactions 3 - 7 ..
            for reject_tx_idx in 3..8 {
                vote_info
                    .reject_txn_votes
                    .insert(reject_tx_idx, StakeAggregator::new());
                // .. add 5 reject votes which form a quorum.
                for authority_idx in 0..5 {
                    vote_info
                        .reject_txn_votes
                        .get_mut(&reject_tx_idx)
                        .unwrap()
                        .add_unique(AuthorityIndex::new_for_test(authority_idx), committee);
                }
            }

            // The block is certified.
            let certified_block = vote_info.take_certified_output(committee).unwrap();
            assert_eq!(certified_block.block.reference(), block.reference());

            // Certified block cannot be taken again.
            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // A transaction in the block is neither rejected nor certified.
        {
            let mut vote_info = VoteInfo::new();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            vote_info.own_reject_txn_votes = Some(vec![]);
            // Add 5 accept votes which form a quorum.
            for i in 0..5 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }
            // For transactions 3 - 5 ..
            for reject_tx_idx in 3..6 {
                vote_info
                    .reject_txn_votes
                    .insert(reject_tx_idx, StakeAggregator::new());
                // .. add 5 reject votes which form a quorum.
                for authority_idx in 0..5 {
                    vote_info
                        .reject_txn_votes
                        .get_mut(&reject_tx_idx)
                        .unwrap()
                        .add_unique(AuthorityIndex::new_for_test(authority_idx), committee);
                }
            }
            // For transaction 6, add 4 reject votes which do not form a quorum.
            vote_info.reject_txn_votes.insert(6, StakeAggregator::new());
            for authority_idx in 0..4 {
                vote_info
                    .reject_txn_votes
                    .get_mut(&6)
                    .unwrap()
                    .add_unique(AuthorityIndex::new_for_test(authority_idx), committee);
            }

            // The block is not certified.
            assert!(vote_info.take_certified_output(committee).is_none());

            // Add 1 more accept vote from a different authority for transaction 6.
            vote_info
                .reject_txn_votes
                .get_mut(&6)
                .unwrap()
                .add_unique(AuthorityIndex::new_for_test(4), committee);

            // The block is now certified.
            let certified_block = vote_info.take_certified_output(committee).unwrap();
            assert_eq!(certified_block.block.reference(), block.reference());

            // Certified block cannot be taken again.
            assert!(vote_info.take_certified_output(committee).is_none());
        }
    }

    #[tokio::test]
    async fn test_certify_basic() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let mut all_blocks = vec![];

        // Round 1: blocks from all authorities are fully connected to the genesis blocks.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layer(1).num_transactions(4).build();
        all_blocks.extend(dag_builder.all_blocks());

        // Round 1: no block is certified yet.
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(certified_blocks.is_empty());

        // Round 2: A, B & C blocks at round 2 are connected to only A, B & C blocks at round 1.
        // A & B blocks reject transaction 2 from the round 1 C block.
        let transactions = (0..4)
            .map(|_| Transaction::new(vec![0_u8; 16]))
            .collect::<Vec<_>>();
        let ancestors = dag_builder
            .all_blocks()
            .iter()
            .filter_map(|b| {
                if b.author().value() < 3 {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let round_1_block_with_rejected_txn = all_blocks[2].clone();
        assert_eq!(round_1_block_with_rejected_txn.author().value(), 2);
        for author in 0..3 {
            let mut block = TestBlock::new(2, author)
                .set_ancestors(ancestors.clone())
                .set_transactions(transactions.clone());
            if author < 2 {
                block = block.set_transaction_votes(vec![BlockTransactionVotes {
                    block_ref: round_1_block_with_rejected_txn.reference(),
                    rejects: vec![2],
                }]);
            }
            all_blocks.push(VerifiedBlock::new_for_test(block.build()));
        }

        // Round 2: A & B round 1 blocks are certified.
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert_eq!(certified_blocks.len(), 2);
        assert_eq!(
            certified_blocks[0].block.reference(),
            all_blocks[0].reference()
        );
        assert_eq!(
            certified_blocks[1].block.reference(),
            all_blocks[1].reference()
        );

        // Round 3: all blocks connected to round 2 blocks and round 1 D block,
        let ancestors = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() == 1 && b.author().value() == 3 {
                    Some(b.reference())
                } else if b.round() == 2 {
                    assert_ne!(b.author().value(), 3);
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(ancestors.len(), 4, "Ancestors {:?}", ancestors);
        let round_2_block_with_rejected_txn = all_blocks.last().unwrap().clone();
        for author in 0..4 {
            let mut block = TestBlock::new(3, author)
                .set_ancestors(ancestors.clone())
                .set_transactions(transactions.clone());
            let mut votes = vec![BlockTransactionVotes {
                block_ref: round_2_block_with_rejected_txn.reference(),
                rejects: vec![2],
            }];
            if author == 3 {
                votes.push(BlockTransactionVotes {
                    block_ref: round_1_block_with_rejected_txn.reference(),
                    rejects: vec![2],
                });
            }
            block = block.set_transaction_votes(votes);
            all_blocks.push(VerifiedBlock::new_for_test(block.build()));
        }

        // Round 3: add another equivocating block from C, that does not contain reject votes.
        let block = TestBlock::new(3, 3)
            .set_ancestors(ancestors.clone())
            .set_transactions(transactions.clone());
        all_blocks.push(VerifiedBlock::new_for_test(block.build()));

        // Round 3: A, B & C round 1 & round 2 blocks are certified.
        //
        // Notably: D at round 1 is not certified because its accept votes are > 1 round higher.
        // And C at round 1 is now certified because of the new reject vote from D at round 3.
        let mut certifier = CertifierState::new(context.clone());
        let mut certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        certified_blocks.sort_by_key(|b| b.block.reference());
        assert_eq!(
            certified_blocks.len(),
            6,
            "Certified blocks {}",
            certified_blocks
                .iter()
                .map(|b| b.block.reference().to_string())
                .join(",")
        );
        assert_eq!(
            certified_blocks[0].block.reference(),
            all_blocks[0].reference()
        );
        assert_eq!(
            certified_blocks[1].block.reference(),
            all_blocks[1].reference()
        );
        assert_eq!(
            certified_blocks[2].block.reference(),
            all_blocks[2].reference()
        );
        assert_eq!(
            certified_blocks[3].block.reference(),
            all_blocks[4].reference()
        );
        assert_eq!(
            certified_blocks[4].block.reference(),
            all_blocks[5].reference()
        );
        assert_eq!(
            certified_blocks[5].block.reference(),
            all_blocks[6].reference()
        );
    }

    #[tokio::test]
    async fn test_certify_with_own_votes() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context.with_authority_index(AuthorityIndex::new_for_test(3)));

        // Round 1: blocks from all authorities are fully connected to the genesis blocks.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layer(1).num_transactions(4).build();
        let round_1_blocks = dag_builder.all_blocks();

        // Round 2: A, B & C blocks at round 2 are connected to only A, B & C blocks at round 1.
        // A & B blocks reject round 1 B block txn 1 and C block txn 2.
        let transactions = (0..4)
            .map(|_| Transaction::new(vec![0_u8; 16]))
            .collect::<Vec<_>>();
        let ancestors = round_1_blocks
            .iter()
            .map(|b| b.reference())
            .collect::<Vec<_>>();
        let mut round_2_blocks = vec![];
        for author in 0..3 {
            let mut block = TestBlock::new(2, author)
                .set_ancestors(ancestors.clone())
                .set_transactions(transactions.clone());
            if author < 2 {
                block = block.set_transaction_votes(vec![
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[1].reference(),
                        rejects: vec![1, 2],
                    },
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[2].reference(),
                        rejects: vec![2, 3],
                    },
                ]);
            }
            round_2_blocks.push(VerifiedBlock::new_for_test(block.build()));
        }

        // No block should be certified with just round 2 blocks, even if their votes reach quorum.
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks = certifier
            .add_voted_blocks(round_2_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(certified_blocks.is_empty());

        // Add round 1 A block.
        let certified_blocks =
            certifier.add_voted_blocks(vec![(round_1_blocks[0].clone(), vec![])]);
        // The block should be certified.
        assert_eq!(certified_blocks.len(), 1);
        assert_eq!(
            certified_blocks[0].block.reference(),
            round_1_blocks[0].reference()
        );

        // Add round 1 B block with transaction 2 that still cannot be determined.
        let certified_blocks =
            certifier.add_voted_blocks(vec![(round_1_blocks[1].clone(), vec![1])]);
        assert!(certified_blocks.is_empty());

        // Add round 1 C block with all transactions determined via voting by this authority (3).
        let certified_blocks =
            certifier.add_voted_blocks(vec![(round_1_blocks[2].clone(), vec![2, 3])]);
        // The block should be certified.
        assert_eq!(certified_blocks.len(), 1);
        assert_eq!(
            certified_blocks[0].block.reference(),
            round_1_blocks[2].reference()
        );
    }

    #[tokio::test]
    async fn test_certify_fully_connected() {
        telemetry_subscribers::init_for_testing();
        let num_authorities: u32 = 7;
        let (context, _) = Context::new_for_test(num_authorities as usize);
        let context = Arc::new(context);

        // Create fully connected blocks up to num_rounds.
        let num_rounds = 20;
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=num_rounds).build();
        let mut all_blocks = dag_builder.all_blocks();

        // All blocks less than num_rounds are expected to be certified.
        let expected_block_refs = all_blocks
            .iter()
            .filter_map(|b| {
                if b.round() < num_rounds {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        // Add the blocks to certifier in random order.
        all_blocks.shuffle(&mut rand::thread_rng());
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());

        // Take the certified blocks and ensure no transaction is rejected.
        for b in &certified_blocks {
            assert!(b.rejected.is_empty());
        }

        // Ensure the certified blocks are the expected ones.
        let certified_block_refs = certified_blocks
            .iter()
            .map(|b| b.block.reference())
            .collect::<BTreeSet<_>>();

        let diff = expected_block_refs
            .difference(&certified_block_refs)
            .collect::<Vec<_>>();
        assert!(diff.is_empty(), "Blocks {:?} are not certified", diff);

        let diff = certified_block_refs
            .difference(&expected_block_refs)
            .collect::<Vec<_>>();
        assert!(
            diff.is_empty(),
            "Certified blocks {:?} are unexpected",
            diff
        );
    }

    // TODO: add reject votes.
    #[tokio::test]
    async fn test_certify_randomized() {
        telemetry_subscribers::init_for_testing();
        let num_authorities: u32 = 7;
        let (context, _) = Context::new_for_test(num_authorities as usize);
        let context = Arc::new(context);

        // Create minimal connected blocks up to num_rounds.
        let num_rounds = 50;
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=num_rounds)
            .min_ancestor_links(false, None)
            .build();
        let all_blocks = dag_builder.all_blocks();

        // Get the certified blocks, which depends on the structure of the minimum connected DAG.
        let mut certifier = CertifierState::new(context.clone());
        let mut expected_certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        expected_certified_blocks.sort_by_key(|b| b.block.reference());

        // Adding all blocks to certifier in random order should still produce the same set of certified blocks.
        for _ in 0..100 {
            // Add the blocks to certifier in random order.
            let mut all_blocks = all_blocks.clone();
            all_blocks.shuffle(&mut rand::thread_rng());
            let mut certifier = CertifierState::new(context.clone());

            // Take the certified blocks.
            let mut actual_certified_blocks = certifier
                .add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
            actual_certified_blocks.sort_by_key(|b| b.block.reference());

            // Ensure the certified blocks are the expected ones.
            assert_eq!(
                actual_certified_blocks.len(),
                expected_certified_blocks.len()
            );
            for (actual, expected) in actual_certified_blocks
                .iter()
                .zip(expected_certified_blocks.iter())
            {
                assert_eq!(actual.block.reference(), expected.block.reference());
                assert_eq!(actual.rejected, expected.rejected);
            }
        }
    }
}
