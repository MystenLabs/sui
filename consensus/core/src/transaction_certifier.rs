// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use consensus_config::{Committee, Stake};
use mysten_common::debug_fatal;
use mysten_metrics::monitored_mpsc::UnboundedSender;
use parking_lot::RwLock;

use crate::{
    block::{BlockTransactionVotes, GENESIS_ROUND},
    block_verifier::BlockVerifier,
    context::Context,
    dag_state::DagState,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    BlockAPI as _, BlockRef, CertifiedBlock, CertifiedBlocksOutput, Round, TransactionIndex,
    VerifiedBlock,
};

/// TransactionCertifier has the following purposes:
/// 1. Certifies transactions and sends them to execute on the fastpath.
/// 2. Keeps track of own votes on transactions, and allows the votes to be retrieved
///    later in core after acceptance of the blocks containing the transactions.
/// 3. Aggregates reject votes on transactions, and allows the aggregated votes
///    to be retrieved during post-commit finalization.
///
/// A transaction is certified if a quorum of authorities in the causal history of a proposed block
/// vote to accept the transaction. Accept votes are implicit in blocks: if a transaction is in
/// the causal history of a block and the block does not vote to reject it, the block
/// is considered to vote to accept the transaction. Transaction finalization are eventually resolved
/// post commit, by checking if there is a certification of the transaction in the causal history
/// of the leader. So only accept votes are only considered if they are in the causal history of own
/// proposed blocks.
///
/// A transaction is rejected if a quorum of authorities vote to reject it. When this happens, it is
/// guaranteed that no validator can observe a certification of the transaction, with <= f malicious
/// stake.
///
/// A block is certified if every transaction in the block is either certified or rejected.
/// TransactionCertifier outputs certified blocks.
///
/// The invariant between TransactionCertifier and post-commit finalization is that if a quorum of
/// authorities certified a transaction for fastpath and executed it, then the transaction
/// must also be finalized post consensus commit. The reverse is not true though, because
/// fastpath execution is only a latency optimization, and not required for correctness.
#[derive(Clone)]
pub(crate) struct TransactionCertifier {
    // The state of blocks being voted on and certified.
    certifier_state: Arc<RwLock<CertifierState>>,
    // The state of the DAG.
    dag_state: Arc<RwLock<DagState>>,
    // An unbounded channel to output certified blocks to Sui consensus block handler.
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

    /// Recovers internal state from blocks in storage.
    /// Since votes are not stored persistently, blocks where votes have not been proposed are re-verified and voted.
    pub(crate) fn recover(&self, block_verifier: &impl BlockVerifier) {
        let mut certifier_state = self.certifier_state.write();
        let dag_state = self.dag_state.read();

        let gc_round = dag_state.gc_round();
        let authorities = certifier_state
            .context
            .committee
            .authorities()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for authority_index in authorities {
            let blocks = dag_state.get_cached_blocks(authority_index, gc_round + 1);
            let voted_blocks = blocks
                .into_iter()
                .map(|b| {
                    if dag_state.is_hard_linked(&b.reference()) {
                        (b, vec![])
                    } else {
                        let reject_transaction_votes = block_verifier
                            .verify_and_vote(b.signed_block())
                            .unwrap_or_else(|e| {
                                panic!("Failed to verify block during recovery: {}", e)
                            });
                        (b, reject_transaction_votes)
                    }
                })
                .collect::<Vec<_>>();
            let certified_blocks = certifier_state.add_voted_blocks(voted_blocks);
            self.send_certified_blocks(certified_blocks);
        }

        certifier_state.update_gc_round(gc_round);
    }

    /// Stores own reject votes on input blocks, and aggregates reject votes from the input blocks.
    /// Newly certified blocks are sent to the fastpath output channel.
    pub(crate) fn add_voted_blocks(
        &self,
        voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        let certified_blocks = self.certifier_state.write().add_voted_blocks(voted_blocks);
        self.send_certified_blocks(certified_blocks);
    }

    /// Aggregates accept votes from the own proposed block.
    /// Newly certified blocks are sent to the fastpath output channel.
    pub(crate) fn add_proposed_block(&self, proposed_block: VerifiedBlock) {
        let certified_blocks = self
            .certifier_state
            .write()
            .add_proposed_block(proposed_block);
        self.send_certified_blocks(certified_blocks);
    }

    // Sends certified blocks to the fastpath output channel.
    fn send_certified_blocks(&self, certified_blocks: Vec<CertifiedBlock>) {
        if certified_blocks.is_empty() {
            return;
        }
        if let Err(e) = self.certified_blocks_sender.send(CertifiedBlocksOutput {
            blocks: certified_blocks,
        }) {
            tracing::warn!("Failed to send certified blocks: {:?}", e);
        }
    }

    /// Retrieves own votes on peer block transactions.
    pub(crate) fn get_own_votes(&self, block_refs: Vec<BlockRef>) -> Vec<BlockTransactionVotes> {
        let mut votes = vec![];
        let certifier_state = self.certifier_state.read();
        for block_ref in block_refs {
            if block_ref.round <= certifier_state.gc_round {
                continue;
            }
            let vote_info = certifier_state.votes.get(&block_ref).unwrap_or_else(|| {
                panic!("Ancestor block {} not found in certifier state", block_ref)
            });
            if !vote_info.own_reject_txn_votes.is_empty() {
                votes.push(BlockTransactionVotes {
                    block_ref,
                    rejects: vote_info.own_reject_txn_votes.clone(),
                });
            }
        }
        votes
    }

    /// Retrieves transactions in the block that have received reject votes, and the total stake of the votes.
    /// Returns None if no information is found for the block.
    pub(crate) fn get_reject_votes(
        &self,
        block_ref: &BlockRef,
    ) -> Option<Vec<(TransactionIndex, Stake)>> {
        let mut reject_votes = vec![];
        for (transaction_index, stake_agg) in self
            .certifier_state
            .read()
            .votes
            .get(block_ref)?
            .reject_txn_votes
            .iter()
        {
            if stake_agg.stake() > 0 {
                reject_votes.push((*transaction_index, stake_agg.stake()));
            }
        }
        Some(reject_votes)
    }

    /// Runs garbage collection on the internal state and updates the GC round for the certifier.
    pub(crate) fn run_gc(&self, gc_round: Round) {
        let mut certifier_state = self.certifier_state.write();
        certifier_state.update_gc_round(gc_round);
    }
}

/// CertifierState keeps track of votes received by each transaction and block,
/// and helps determine if votes reach a quorum. Reject votes can start accumulating
/// even before the target block is received by this authority.
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
        vote_info.own_reject_txn_votes = reject_txn_votes;

        let mut certified_blocks = vec![];

        // Update reject votes from the input block.
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
            // Check if the target block is now certified after including the reject votes.
            // NOTE: votes can already exist for the target block and its transactions.
            if let Some(certified_block) = vote_info.take_certified_output(&self.context.committee)
            {
                certified_blocks.push(certified_block);
            }
        }

        certified_blocks
    }

    fn add_proposed_block(&mut self, proposed_block: VerifiedBlock) -> Vec<CertifiedBlock> {
        if proposed_block.round() <= self.gc_round + 2 {
            // Skip additional certification if transactions that can be certified have already been GC'ed.
            return vec![];
        }

        // Vote entry for the proposed block must already exist.
        assert!(
            self.votes.contains_key(&proposed_block.reference()),
            "Proposed block {} not found in certifier state",
            proposed_block.reference()
        );

        let mut certified_blocks = vec![];
        for voting_ancestor in proposed_block.ancestors() {
            // Votes are 1 round before the proposed block.
            if voting_ancestor.round + 1 != proposed_block.round() {
                continue;
            }
            let Some(voting_info) = self.votes.get(voting_ancestor) else {
                debug_fatal!("voting info not found for ancestor {}", voting_ancestor);
                continue;
            };
            let Some(voting_block) = voting_info.block.clone() else {
                debug_fatal!("voting block not found for ancestor {}", voting_ancestor);
                continue;
            };
            for target_ancestor in voting_block.ancestors() {
                // Target blocks are 1 round before the voting block.
                if target_ancestor.round + 1 != voting_block.round() {
                    continue;
                }
                let Some(target_vote_info) = self.votes.get_mut(target_ancestor) else {
                    debug_fatal!("voting info not found for ancestor {}", target_ancestor);
                    continue;
                };
                target_vote_info
                    .accept_block_votes
                    .add_unique(voting_block.author(), &self.context.committee);
                // Check if the target block is now certified after including the accept votes.
                if let Some(certified_block) =
                    target_vote_info.take_certified_output(&self.context.committee)
                {
                    certified_blocks.push(certified_block);
                }
            }
        }

        certified_blocks
    }

    /// Updates the GC round and cleans up obsolete internal state.
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
    // This field is written when the block is first received and its transactions are voted on.
    // It is read from core after the block is accepted.
    own_reject_txn_votes: Vec<TransactionIndex>,
    // Accumulates implicit accept votes for the block and all transactions.
    accept_block_votes: StakeAggregator<QuorumThreshold>,
    // Accumulates reject votes per transaction in this block.
    reject_txn_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    // Whether this block has been certified already.
    is_certified: bool,
}

impl VoteInfo {
    // If this block can now be certified, returns the output.
    // Otherwise, returns None.
    fn take_certified_output(&mut self, committee: &Committee) -> Option<CertifiedBlock> {
        if self.is_certified {
            // Skip if already certified.
            return None;
        }
        let Some(block) = self.block.as_ref() else {
            // Skip if the content of the block has not been received.
            return None;
        };
        if !self.accept_block_votes.reached_threshold(committee) {
            // Skip if the block is not certified.
            return None;
        }
        let mut rejected = vec![];
        for (idx, reject_txn_votes) in &self.reject_txn_votes {
            // The transaction is voted to be rejected.
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
            // Also, the total accept votes of a transactions is undercounted here.
            // If a block has accept votes from a quorum of authorities A, B and C, but one transaction
            // has a reject vote from D, the transaction and block are technically certified
            // and can be sent to fastpath. However, the computation here will not certify the transaction
            // or the block. This is still fine because the fastpath certification is optional.
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
        Self {
            block: None,
            own_reject_txn_votes: vec![],
            accept_block_votes: StakeAggregator::new(),
            reject_txn_votes: BTreeMap::new(),
            is_certified: false,
        }
    }
}

#[cfg(test)]
mod test {
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
            let mut vote_info = VoteInfo::default();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // Accept votes but not enough.
        {
            let mut vote_info = VoteInfo::default();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
            for i in 0..4 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // Enough accept votes but no block.
        {
            let mut vote_info = VoteInfo::default();
            for i in 0..5 {
                vote_info
                    .accept_block_votes
                    .add_unique(AuthorityIndex::new_for_test(i), committee);
            }

            assert!(vote_info.take_certified_output(committee).is_none());
        }

        // A quorum of accept votes and block exists.
        {
            let mut vote_info = VoteInfo::default();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
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
            let mut vote_info = VoteInfo::default();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
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
            let mut vote_info = VoteInfo::default();
            let block = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
            vote_info.block = Some(block.clone());
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

        // GIVEN: Round 1: blocks from all authorities are fully connected to the genesis blocks.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layer(1).num_transactions(4).build();
        let round_1_blocks = dag_builder.all_blocks();
        let mut all_blocks = round_1_blocks.clone();

        // THEN: Round 1: no block can be certified yet.
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks = certifier
            .add_voted_blocks(round_1_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(certified_blocks.is_empty());

        // GIVEN: Round 2: A, B & C blocks at round 2 are connected to only A, B & C blocks at round 1.
        // AND: A & B blocks reject transaction 1 from the round 1 B block.
        // AND: A, B & C blocks reject transaction 2 from the round 1 C block.
        let transactions = (0..4)
            .map(|_| Transaction::new(vec![0_u8; 16]))
            .collect::<Vec<_>>();
        let ancestors = round_1_blocks
            .iter()
            .filter_map(|b| {
                if b.author().value() < 3 {
                    Some(b.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for author in 0..3 {
            let mut block = TestBlock::new(2, author)
                .set_ancestors(ancestors.clone())
                .set_transactions(transactions.clone());
            let mut votes = vec![];
            for i in 0..(3 - author) {
                let j = author + i;
                if j == 0 {
                    // Do not reject transaction 0 from the round 1 A block.
                    continue;
                }
                votes.push(BlockTransactionVotes {
                    block_ref: round_1_blocks[j as usize].reference(),
                    rejects: vec![j as u16],
                });
            }
            block = block.set_transaction_votes(votes);
            all_blocks.push(VerifiedBlock::new_for_test(block.build()));
        }

        // THEN: Round 2: no block can be certified yet.
        let mut certifier = CertifierState::new(context.clone());
        let certified_blocks =
            certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(certified_blocks.is_empty());

        // GIVEN: Round 3: all blocks connected to round 2 blocks and round 1 D block,
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
        let mut round_3_blocks = vec![];
        for author in 0..4 {
            let block = TestBlock::new(3, author)
                .set_ancestors(ancestors.clone())
                .set_transactions(transactions.clone());
            round_3_blocks.push(VerifiedBlock::new_for_test(block.build()));
        }

        // THEN: Round 3: with 1 round 3 block, A & C round 1 blocks are certified.
        let mut certifier = CertifierState::new(context.clone());
        certifier.add_voted_blocks(all_blocks.iter().map(|b| (b.clone(), vec![])).collect());
        let proposed_block = round_3_blocks.pop().unwrap();
        let mut certified_blocks =
            certifier.add_voted_blocks(vec![(proposed_block.clone(), vec![])]);
        certified_blocks.extend(certifier.add_proposed_block(proposed_block));
        assert_eq!(
            certified_blocks.len(),
            2,
            "Certified blocks {:?}",
            certified_blocks
                .iter()
                .map(|b| b.block.reference().to_string())
                .join(",")
        );
        assert_eq!(
            certified_blocks[0].block.reference(),
            round_1_blocks[0].reference()
        );
        assert!(certified_blocks[0].rejected.is_empty());
        assert_eq!(
            certified_blocks[1].block.reference(),
            round_1_blocks[2].reference()
        );
        assert_eq!(certified_blocks[1].rejected, vec![2]);
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
