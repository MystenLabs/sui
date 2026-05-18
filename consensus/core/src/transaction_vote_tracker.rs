// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use consensus_config::Stake;
use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_common::ZipDebugEqIteratorExt;
use parking_lot::RwLock;
use tracing::info;

use crate::{
    BlockAPI as _, VerifiedBlock,
    block::{BlockTransactionVotes, GENESIS_ROUND},
    block_verifier::BlockVerifier,
    context::Context,
    dag_state::DagState,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

/// TransactionVoteTracker has the following purposes:
/// 1. Keeps track of own votes on transactions, and allows the votes to be retrieved
///    later in core after acceptance of the blocks containing the transactions.
/// 2. Aggregates reject votes on transactions, and allows the aggregated votes
///    to be retrieved during post-commit finalization.
///
/// A transaction is rejected if a quorum of authorities vote to reject it. When this happens, it is
/// guaranteed that no validator can observe a certification of the transaction, with <= f malicious
/// stake.
#[derive(Clone)]
pub struct TransactionVoteTracker {
    // The state of blocks being voted on.
    vote_tracker_state: Arc<RwLock<VoteTrackerState>>,
    // Verify transactions during recovery.
    block_verifier: Arc<dyn BlockVerifier>,
    // The state of the DAG.
    dag_state: Arc<RwLock<DagState>>,
}

impl TransactionVoteTracker {
    pub fn new(
        context: Arc<Context>,
        block_verifier: Arc<dyn BlockVerifier>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        Self {
            vote_tracker_state: Arc::new(RwLock::new(VoteTrackerState::new(context))),
            block_verifier,
            dag_state,
        }
    }

    /// Recovers all blocks from DB after the given round.
    ///
    /// This is useful for initializing the vote tracker state
    /// for future commits and block proposals.
    pub(crate) fn recover_blocks_after_round(&self, after_round: Round) {
        let context = self.vote_tracker_state.read().context.clone();
        if !context.protocol_config.transaction_voting_enabled() {
            info!("Skipping vote tracker recovery in non-mysticeti fast path mode");
            return;
        }

        let store = self.dag_state.read().store().clone();

        let recovery_start_round = after_round + 1;
        info!(
            "Recovering vote tracker state from round {}",
            recovery_start_round,
        );

        let authorities = context
            .committee
            .authorities()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for authority_index in authorities {
            let blocks = store
                .scan_blocks_by_author(authority_index, recovery_start_round)
                .unwrap();
            info!(
                "Recovered and voting on {} blocks from authority {} {}",
                blocks.len(),
                authority_index,
                context.committee.authority(authority_index).hostname
            );
            self.recover_and_vote_on_blocks(blocks);
        }
    }

    /// Recovers and potentially votes on the given blocks.
    ///
    /// Because own votes on blocks are not stored, during recovery it is necessary to vote on
    /// input blocks that are above GC round and have not been included before, which can be
    /// included in a future proposed block.
    ///
    /// In addition, add_voted_blocks() will eventually process reject votes contained in the input blocks.
    pub(crate) fn recover_and_vote_on_blocks(&self, blocks: Vec<VerifiedBlock>) {
        let context = self.vote_tracker_state.read().context.clone();
        let should_vote_blocks = {
            let dag_state = self.dag_state.read();
            let gc_round = dag_state.gc_round();
            blocks
                .iter()
                // Must make sure the block is above GC round before calling has_been_included().
                .map(|b| b.round() > gc_round && !dag_state.has_been_included(&b.reference()))
                .collect::<Vec<_>>()
        };
        let voted_blocks = blocks
            .into_iter()
            .zip_debug_eq(should_vote_blocks)
            .map(|(b, should_vote)| {
                if !should_vote {
                    // Voting is unnecessary for blocks already included in own proposed blocks,
                    // or outside of local DAG GC bound.
                    (b, vec![])
                } else {
                    // Voting is needed for blocks above GC round and not yet included in own proposed blocks.
                    // A block proposal can include the input block later and retries own votes on it.
                    let reject_transaction_votes =
                        self.block_verifier.vote(&b).unwrap_or_else(|e| {
                            panic!(
                                "Failed to vote on block {} (own_index={}) during recovery: {}",
                                b.reference(),
                                context.own_index,
                                e
                            )
                        });
                    (b, reject_transaction_votes)
                }
            })
            .collect::<Vec<_>>();
        self.vote_tracker_state
            .write()
            .add_voted_blocks(voted_blocks);
    }

    /// Stores own reject votes on input blocks, and aggregates reject votes from the input blocks.
    pub fn add_voted_blocks(&self, voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>) {
        self.vote_tracker_state
            .write()
            .add_voted_blocks(voted_blocks);
    }

    /// Retrieves own votes on peer block transactions.
    pub(crate) fn get_own_votes(&self, block_refs: Vec<BlockRef>) -> Vec<BlockTransactionVotes> {
        let mut votes = vec![];
        let vote_tracker_state = self.vote_tracker_state.read();
        for block_ref in block_refs {
            if block_ref.round <= vote_tracker_state.gc_round {
                continue;
            }
            let vote_info = vote_tracker_state.votes.get(&block_ref).unwrap_or_else(|| {
                panic!(
                    "Ancestor block {} not found in vote tracker state",
                    block_ref
                )
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
    /// TransactionIndex not included in the output has no reject votes.
    /// Returns None if no information is found for the block.
    pub(crate) fn get_reject_votes(
        &self,
        block_ref: &BlockRef,
    ) -> Option<Vec<(TransactionIndex, Stake)>> {
        let accumulated_reject_votes = self
            .vote_tracker_state
            .read()
            .votes
            .get(block_ref)?
            .reject_txn_votes
            .iter()
            .map(|(idx, stake_agg)| (*idx, stake_agg.stake()))
            .collect::<Vec<_>>();
        Some(accumulated_reject_votes)
    }

    /// Runs garbage collection on the internal state by removing data for blocks <= gc_round,
    /// and updates the GC round for the vote tracker.
    ///
    /// IMPORTANT: the gc_round used here can trail the latest gc_round from DagState.
    /// This is because the gc round here is determined by CommitFinalizer, which needs to process
    /// commits before the latest commit in DagState. Reject votes received by transactions below
    /// local DAG gc_round may still need to be accessed from CommitFinalizer.
    pub(crate) fn run_gc(&self, gc_round: Round) {
        let dag_state_gc_round = self.dag_state.read().gc_round();
        assert!(
            gc_round <= dag_state_gc_round,
            "TransactionVoteTracker cannot GC higher than DagState GC round ({} > {})",
            gc_round,
            dag_state_gc_round
        );
        self.vote_tracker_state.write().update_gc_round(gc_round);
    }
}

/// VoteTrackerState keeps track of votes received by each transaction and block,
/// and helps determine if votes reach a quorum. Reject votes can start accumulating
/// even before the target block is received by this authority.
struct VoteTrackerState {
    context: Arc<Context>,

    // Maps received blocks' refs to votes on those blocks from other blocks.
    // Even if a block has no reject votes on its transactions, it still has an entry here.
    votes: BTreeMap<BlockRef, VoteInfo>,

    // Highest round where blocks are GC'ed.
    gc_round: Round,
}

impl VoteTrackerState {
    fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            votes: BTreeMap::new(),
            gc_round: GENESIS_ROUND,
        }
    }

    fn add_voted_blocks(&mut self, voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)>) {
        for (voted_block, reject_txn_votes) in voted_blocks {
            self.add_voted_block(voted_block, reject_txn_votes);
        }
    }

    fn add_voted_block(
        &mut self,
        voted_block: VerifiedBlock,
        reject_txn_votes: Vec<TransactionIndex>,
    ) {
        if voted_block.round() <= self.gc_round {
            // Ignore the block and own votes, since they are outside of vote tracker GC bound.
            return;
        }

        // Count own reject votes against each peer authority.
        let peer_hostname = &self
            .context
            .committee
            .authority(voted_block.author())
            .hostname;
        self.context
            .metrics
            .node_metrics
            .certifier_own_reject_votes
            .with_label_values(&[peer_hostname])
            .inc_by(reject_txn_votes.len() as u64);

        // Initialize the entry for the voted block.
        let vote_info = self.votes.entry(voted_block.reference()).or_default();
        if vote_info.block.is_some() {
            // Input block has already been processed and added to the state.
            return;
        }
        vote_info.block = Some(voted_block.clone());
        vote_info.own_reject_txn_votes = reject_txn_votes;

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
        }
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

        self.context
            .metrics
            .node_metrics
            .certifier_gc_round
            .set(self.gc_round as i64);
    }
}

/// VoteInfo keeps track of votes received for each transaction of this block,
/// possibly even before the block is received by this authority.
#[derive(Default)]
struct VoteInfo {
    // Content of the block.
    // None if the blocks has not been received.
    block: Option<VerifiedBlock>,
    // Rejection votes by this authority on this block.
    // This field is written when the block is first received and its transactions are voted on.
    // It is read from core after the block is accepted.
    own_reject_txn_votes: Vec<TransactionIndex>,
    // Accumulates reject votes per transaction in this block.
    reject_txn_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use consensus_config::{AuthorityIndex, Parameters};

    use crate::{
        TestBlock, Transaction, VerifiedBlock, block::BlockTransactionVotes, context::Context,
        metrics::test_metrics,
    };

    use super::*;

    // 4 authorities with stakes [1, 2, 3, 4], total 10.
    #[tokio::test]
    async fn test_reject_vote_tracking() {
        telemetry_subscribers::init_for_testing();
        let (committee, _keypairs) =
            consensus_config::local_committee_and_keys(0, vec![1, 2, 3, 4]);
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = Arc::new(Context::new(
            0,
            Some(AuthorityIndex::new_for_test(0)),
            committee,
            Parameters {
                db_path: temp_dir.path().to_path_buf(),
                ..Default::default()
            },
            consensus_config::ConsensusProtocolConfig::for_testing(),
            test_metrics(),
            Arc::new(crate::Clock::default()),
        ));

        let transactions = vec![Transaction::new(vec![0u8; 16]); 4];

        // Round 1: create a block from each authority.
        let round_1_blocks: Vec<VerifiedBlock> = (0..4)
            .map(|author| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(1, author)
                        .set_transactions(transactions.clone())
                        .build(),
                )
            })
            .collect();

        // Add round 1 blocks with own reject votes:
        // - reject txn 0 of block from authority 0
        // - reject txns 1 and 2 of block from authority 1
        // - no rejects for blocks from authorities 2 and 3
        let mut state = VoteTrackerState::new(context.clone());
        state.add_voted_blocks(vec![
            (round_1_blocks[0].clone(), vec![0]),
            (round_1_blocks[1].clone(), vec![1, 2]),
            (round_1_blocks[2].clone(), vec![]),
            (round_1_blocks[3].clone(), vec![]),
        ]);

        // Verify own reject votes are stored correctly.
        let vote_info_0 = state.votes.get(&round_1_blocks[0].reference()).unwrap();
        assert_eq!(vote_info_0.own_reject_txn_votes, vec![0]);
        let vote_info_1 = state.votes.get(&round_1_blocks[1].reference()).unwrap();
        assert_eq!(vote_info_1.own_reject_txn_votes, vec![1, 2]);
        let vote_info_2 = state.votes.get(&round_1_blocks[2].reference()).unwrap();
        assert!(vote_info_2.own_reject_txn_votes.is_empty());

        // No reject votes have been aggregated yet (round 1 blocks have no transaction_votes).
        assert!(vote_info_0.reject_txn_votes.is_empty());
        assert!(vote_info_1.reject_txn_votes.is_empty());

        // Round 2: authorities 0, 1, 2 create blocks that reject transactions in round 1 blocks.
        let ancestors: Vec<BlockRef> = round_1_blocks.iter().map(|b| b.reference()).collect();

        // Authority 0 (stake 1) rejects txn 0 of block[0] and txn 1 of block[1].
        let block_r2_a0 = VerifiedBlock::new_for_test(
            TestBlock::new(2, 0)
                .set_ancestors_raw(ancestors.clone())
                .set_transactions(transactions.clone())
                .set_transaction_votes(vec![
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[0].reference(),
                        rejects: vec![0],
                    },
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[1].reference(),
                        rejects: vec![1],
                    },
                ])
                .build(),
        );

        // Authority 1 (stake 2) rejects txn 0 of block[0] and txns 1,2 of block[1].
        let block_r2_a1 = VerifiedBlock::new_for_test(
            TestBlock::new(2, 1)
                .set_ancestors_raw(ancestors.clone())
                .set_transactions(transactions.clone())
                .set_transaction_votes(vec![
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[0].reference(),
                        rejects: vec![0],
                    },
                    BlockTransactionVotes {
                        block_ref: round_1_blocks[1].reference(),
                        rejects: vec![1, 2],
                    },
                ])
                .build(),
        );

        // Authority 2 (stake 3) rejects txn 2 of block[1] only.
        let block_r2_a2 = VerifiedBlock::new_for_test(
            TestBlock::new(2, 2)
                .set_ancestors_raw(ancestors.clone())
                .set_transactions(transactions.clone())
                .set_transaction_votes(vec![BlockTransactionVotes {
                    block_ref: round_1_blocks[1].reference(),
                    rejects: vec![2],
                }])
                .build(),
        );

        state.add_voted_blocks(vec![
            (block_r2_a0, vec![]),
            (block_r2_a1, vec![]),
            (block_r2_a2, vec![]),
        ]);

        // Verify aggregated reject votes for block[0]:
        // txn 0: authority 0 (stake 1) + authority 1 (stake 2) = 3
        let reject_votes_0 = &state
            .votes
            .get(&round_1_blocks[0].reference())
            .unwrap()
            .reject_txn_votes;
        assert_eq!(reject_votes_0.len(), 1);
        assert_eq!(reject_votes_0.get(&0).unwrap().stake(), 3);

        // Verify aggregated reject votes for block[1]:
        // txn 1: authority 0 (stake 1) + authority 1 (stake 2) = 3
        // txn 2: authority 1 (stake 2) + authority 2 (stake 3) = 5
        let reject_votes_1 = &state
            .votes
            .get(&round_1_blocks[1].reference())
            .unwrap()
            .reject_txn_votes;
        assert_eq!(reject_votes_1.len(), 2);
        assert_eq!(reject_votes_1.get(&1).unwrap().stake(), 3);
        assert_eq!(reject_votes_1.get(&2).unwrap().stake(), 5);

        // block[2] and block[3] have no reject votes from others.
        let reject_votes_2 = &state
            .votes
            .get(&round_1_blocks[2].reference())
            .unwrap()
            .reject_txn_votes;
        assert!(reject_votes_2.is_empty());
    }
}
