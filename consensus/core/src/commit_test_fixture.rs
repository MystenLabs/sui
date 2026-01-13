// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test fixture for commit-related tests.
//! Used by both commit_finalizer.rs tests and randomized_tests.rs.

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use consensus_types::block::TransactionIndex;
use mysten_metrics::monitored_mpsc::unbounded_channel;
use parking_lot::RwLock;

use crate::{
    block::VerifiedBlock,
    block_manager::BlockManager,
    block_verifier::NoopBlockVerifier,
    commit::{CommittedSubDag, DecidedLeader},
    commit_finalizer::CommitFinalizer,
    context::Context,
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
    linearizer::Linearizer,
    storage::mem_store::MemStore,
    transaction_certifier::TransactionCertifier,
    universal_committer::{
        UniversalCommitter, universal_committer_builder::UniversalCommitterBuilder,
    },
};

/// A test fixture that provides all the components needed for testing commit processing,
/// similar to the actual logic in Core::try_commit() and CommitFinalizer::run().
pub(crate) struct CommitTestFixture {
    pub(crate) context: Arc<Context>,
    pub(crate) committer: UniversalCommitter,
    pub(crate) linearizer: Linearizer,
    pub(crate) transaction_certifier: TransactionCertifier,
    pub(crate) commit_finalizer: CommitFinalizer,

    dag_state: Arc<RwLock<DagState>>,
    block_manager: BlockManager,
}

impl CommitTestFixture {
    /// Creates a new CommitTestFixture from a context.
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let leader_schedule = Arc::new(LeaderSchedule::new(
            context.clone(),
            LeaderSwapTable::default(),
        ));
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));

        // Create committer with pipelining and only 1 leader per leader round
        let committer =
            UniversalCommitterBuilder::new(context.clone(), leader_schedule, dag_state.clone())
                .with_pipeline(true)
                .build();

        let block_manager = BlockManager::new(context.clone(), dag_state.clone());

        let linearizer = Linearizer::new(context.clone(), dag_state.clone());
        let (blocks_sender, _blocks_receiver) = unbounded_channel("consensus_block_output");
        let transaction_certifier = TransactionCertifier::new(
            context.clone(),
            Arc::new(NoopBlockVerifier {}),
            dag_state.clone(),
            blocks_sender,
        );
        let (commit_sender, _commit_receiver) = unbounded_channel("consensus_commit_output");
        let commit_finalizer = CommitFinalizer::new(
            context.clone(),
            dag_state.clone(),
            transaction_certifier.clone(),
            commit_sender,
        );

        Self {
            context,
            dag_state,
            committer,
            block_manager,
            linearizer,
            transaction_certifier,
            commit_finalizer,
        }
    }

    /// Creates a new CommitTestFixture with more options.
    pub(crate) fn with_options(
        num_authorities: usize,
        authority_index: u32,
        gc_depth: Option<u32>,
    ) -> Self {
        Self::new(Self::context_with_options(
            num_authorities,
            authority_index,
            gc_depth,
        ))
    }

    pub(crate) fn context_with_options(
        num_authorities: usize,
        authority_index: u32,
        gc_depth: Option<u32>,
    ) -> Arc<Context> {
        let (mut context, _keys) = Context::new_for_test(num_authorities);
        if let Some(gc_depth) = gc_depth {
            context
                .protocol_config
                .set_consensus_gc_depth_for_testing(gc_depth);
        }
        Arc::new(context.with_authority_index(AuthorityIndex::new_for_test(authority_index)))
    }

    // Adds the blocks to the transaction certifier and then tries to accept them via BlockManager.
    /// This registers the blocks for reject vote tracking (with no reject votes).
    pub(crate) fn try_accept_blocks(&mut self, blocks: Vec<VerifiedBlock>) {
        self.transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        self.block_manager.try_accept_blocks(blocks);
    }

    // Adds the blocks to the transaction certifier and then tries to accept them via BlockManager.
    /// This registers the blocks for reject vote tracking (with own reject votes).
    pub(crate) fn try_accept_blocks_with_own_votes(
        &mut self,
        blocks_and_votes: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        let blocks = blocks_and_votes.iter().map(|(b, _)| b.clone()).collect();
        self.transaction_certifier
            .add_voted_blocks(blocks_and_votes);
        self.block_manager.try_accept_blocks(blocks);
    }

    /// Adds blocks to the transaction certifier and dag state.
    /// This registers the blocks for reject vote tracking (with no reject votes).
    pub(crate) fn add_blocks(&self, blocks: Vec<VerifiedBlock>) {
        let blocks_and_votes = blocks.iter().map(|b| (b.clone(), vec![])).collect();
        self.transaction_certifier
            .add_voted_blocks(blocks_and_votes);
        self.dag_state.write().accept_blocks(blocks);
    }

    pub(crate) fn add_blocks_with_own_votes(
        &self,
        blocks_and_votes: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        let blocks = blocks_and_votes.iter().map(|(b, _)| b.clone()).collect();
        self.transaction_certifier
            .add_voted_blocks(blocks_and_votes);
        self.dag_state.write().accept_blocks(blocks);
    }

    /// Checks if the block manager has no suspended blocks.
    pub(crate) fn has_no_suspended_blocks(&self) -> bool {
        self.block_manager.is_empty()
    }

    /// Process decided leaders through linearizer and commit finalizer,
    /// similar to Core::try_commit() and CommitFinalizer::run().
    ///
    /// This extracts leader blocks from DecidedLeader::Commit, creates CommittedSubDags
    /// via the linearizer, and processes them through the commit finalizer.
    pub(crate) async fn process_commits(
        &mut self,
        sequence: Vec<DecidedLeader>,
    ) -> Vec<CommittedSubDag> {
        // Extract leader blocks from DecidedLeader::Commit (skip Skip decisions)
        let leaders: Vec<VerifiedBlock> = sequence
            .into_iter()
            .filter_map(|d| match d {
                DecidedLeader::Commit(block, _) => Some(block),
                DecidedLeader::Skip(_) => None,
            })
            .collect();

        if leaders.is_empty() {
            return vec![];
        }

        // Use linearizer to create CommittedSubDag
        let committed_sub_dags = self.linearizer.handle_commit(leaders);

        // Process through commit finalizer
        let mut finalized_commits = vec![];
        for mut subdag in committed_sub_dags {
            subdag.decided_with_local_blocks = true;
            let finalized = self.commit_finalizer.process_commit(subdag).await;
            finalized_commits.extend(finalized);
        }

        finalized_commits
    }
}
