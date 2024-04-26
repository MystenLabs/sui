// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, BlockTimestampMs, Round, VerifiedBlock},
    commit::{Commit, CommitIndex, CommittedSubDag, TrustedCommit},
    dag_state::DagState,
};

/// Expand a committed sequence of leader into a sequence of sub-dags.
#[derive(Clone)]
pub(crate) struct Linearizer {
    /// In memory block store representing the dag state
    dag_state: Arc<RwLock<DagState>>,
}

impl Linearizer {
    pub(crate) fn new(dag_state: Arc<RwLock<DagState>>) -> Self {
        Self { dag_state }
    }

    /// Collect the sub-dag from a specific leader excluding any duplicates or
    /// blocks that have already been committed (within previous sub-dags).
    fn collect_sub_dag(
        &mut self,
        leader_block: VerifiedBlock,
        last_commit_index: CommitIndex,
        last_commit_timestamp_ms: BlockTimestampMs,
        last_committed_rounds: Vec<Round>,
    ) -> CommittedSubDag {
        let mut to_commit = Vec::new();
        let mut committed = HashSet::new();

        let timestamp_ms = leader_block.timestamp_ms().max(last_commit_timestamp_ms);
        let leader_block_ref = leader_block.reference();
        let mut buffer = vec![leader_block];
        assert!(committed.insert(leader_block_ref));

        let dag_state = self.dag_state.read();
        while let Some(x) = buffer.pop() {
            to_commit.push(x.clone());

            let ancestors: Vec<VerifiedBlock> = dag_state
                .get_blocks(
                    &x.ancestors()
                        .iter()
                        .copied()
                        .filter(|ancestor| {
                            // We skip the block if we already committed it or we reached a
                            // round that we already committed.
                            !committed.contains(ancestor)
                                && last_committed_rounds[ancestor.author] < ancestor.round
                        })
                        .collect::<Vec<_>>(),
                )
                .into_iter()
                .map(|ancestor_opt| {
                    ancestor_opt.expect("We should have all uncommitted blocks in dag state.")
                })
                .collect();

            for ancestor in ancestors {
                buffer.push(ancestor.clone());
                assert!(committed.insert(ancestor.reference()));
            }
        }
        CommittedSubDag::new(
            leader_block_ref,
            to_commit,
            timestamp_ms,
            last_commit_index + 1,
        )
    }

    // This function should be called whenever a new commit is observed. This will
    // iterate over the sequence of committed leaders and produce a list of committed
    // sub-dags.
    pub(crate) fn handle_commit(
        &mut self,
        committed_leaders: Vec<VerifiedBlock>,
    ) -> Vec<CommittedSubDag> {
        let mut committed_sub_dags = vec![];
        for leader_block in committed_leaders {
            // Grab latest commit state from dag state
            let dag_state = self.dag_state.read();
            let last_commit_index = dag_state.last_commit_index();
            let last_commit_digest = dag_state.last_commit_digest();
            let last_commit_timestamp_ms = dag_state.last_commit_timestamp_ms();
            let mut last_committed_rounds = dag_state.last_committed_rounds();
            drop(dag_state);

            // Collect the sub-dag generated using each of these leaders.
            let mut sub_dag = self.collect_sub_dag(
                leader_block,
                last_commit_index,
                last_commit_timestamp_ms,
                last_committed_rounds.clone(),
            );

            // [Optional] sort the sub-dag using a deterministic algorithm.
            sub_dag.sort();

            // Summarize CommittedSubDag into Commit.
            let commit = Commit::new(
                sub_dag.commit_index,
                last_commit_digest,
                sub_dag.timestamp_ms,
                sub_dag.leader,
                sub_dag
                    .blocks
                    .iter()
                    .map(|block| {
                        let block_ref = block.reference();
                        last_committed_rounds[block_ref.author.value()] = block_ref.round;
                        block_ref
                    })
                    .collect(),
            );
            let serialized = commit
                .serialize()
                .unwrap_or_else(|e| panic!("Failed to serialize commit: {}", e));
            let commit = TrustedCommit::new_trusted(commit, serialized);

            // Buffer commit in dag state for persistence later.
            // This also updates the last committed rounds.
            self.dag_state.write().add_commit(commit.clone());

            committed_sub_dags.push(sub_dag);
        }

        // Committed blocks must be persisted to storage before sending them to Sui and executing
        // their transactions.
        // Commit metadata can be persisted more lazily because they are recoverable. Uncommitted
        // blocks can wait to persist too.
        // But for simplicity, all unpersisted blocks and commits are flushed to storage.
        if !committed_sub_dags.is_empty() {
            self.dag_state.write().flush();
        }
        committed_sub_dags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commit::{CommitAPI as _, CommitDigest, DEFAULT_WAVE_LENGTH},
        context::Context,
        leader_schedule::{LeaderSchedule, LeaderSwapTable},
        storage::mem_store::MemStore,
        test_dag::{get_all_uncommitted_leader_blocks},
    };
    use crate::block::BlockRef;
    use crate::test_dag_builder::DagBuilder;

    #[test]
    fn test_handle_commit() {
        telemetry_subscribers::init_for_testing();
        let num_authorities = 4;
        let context = Arc::new(Context::new_for_test(num_authorities).0);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut linearizer = Linearizer::new(dag_state.clone());
        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds: u32 = 10;
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=num_rounds).persist_layers(dag_state.clone());

        let leaders = get_all_uncommitted_leader_blocks(
            dag_state.clone(),
            leader_schedule,
            num_rounds,
            DEFAULT_WAVE_LENGTH,
            false,
            1,
        );

        let commits = linearizer.handle_commit(leaders.clone());
        for (idx, subdag) in commits.into_iter().enumerate() {
            tracing::info!("{subdag:?}");
            assert_eq!(subdag.leader, leaders[idx].reference());
            assert_eq!(subdag.timestamp_ms, leaders[idx].timestamp_ms());
            if idx == 0 {
                // First subdag includes the leader block plus all ancestor blocks
                // of the leader minus the genesis round blocks
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * (DEFAULT_WAVE_LENGTH - 1) as usize) + 1
                );
            } else {
                // Every subdag after will be missing the leader block from the previous
                // committed subdag
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * DEFAULT_WAVE_LENGTH as usize)
                );
            }
            for block in subdag.blocks.iter() {
                assert!(block.round() <= leaders[idx].round());
            }
            assert_eq!(subdag.commit_index, idx as CommitIndex + 1);
        }
    }

    #[test]
    fn test_handle_already_committed() {
        telemetry_subscribers::init_for_testing();
        let num_authorities = 4;
        let context = Arc::new(Context::new_for_test(num_authorities).0);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());
        let mut linearizer = Linearizer::new(dag_state.clone());
        let wave_length = DEFAULT_WAVE_LENGTH;

        let leader_round_wave_1 = 3;
        let leader_round_wave_2 = leader_round_wave_1 + wave_length;

        // Build a Dag from round 1..=6
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=leader_round_wave_2).build().persist_layers(dag_state.clone());

        // Now retrieve all the blocks up to round leader_round_wave_1 - 1
        // And then only the leader of round leader_round_wave_1
        // Also store those to DagState
        let blocks = dag_builder
            .blocks
            .iter()
            .flat_map(|(block_ref ,block)|{
            if block_ref.round < leader_round_wave_1 || (block_ref.round == leader_round_wave_1 && block_ref.author == leader_schedule.elect_leader(leader_round_wave_1, 0)) {
                Some(block.reference())
            } else {
                None
            }
        }).collect::<Vec<BlockRef>>();


        let leaders = get_all_uncommitted_leader_blocks(
            dag_state.clone(),
            leader_schedule.clone(),
            leader_round_wave_1,
            wave_length,
            false,
            1,
        );

        let first_leader = leaders[0].clone();
        let mut last_commit_index = 1;
        let first_commit_data = TrustedCommit::new_for_test(
            last_commit_index,
            CommitDigest::MIN,
            0,
            first_leader.reference(),
            blocks.clone(),
        );
        dag_state.write().add_commit(first_commit_data);

        let mut blocks = dag_builder
            .blocks
            .iter()
            .flat_map(|(block_ref ,block)|{
                // Add all nonleader blocks from round 3
                if (block_ref.round == leader_round_wave_1 && block_ref.author != leader_schedule.elect_leader(leader_round_wave_1, 0)) ||
                    (block_ref.round > leader_round_wave_1 && block_ref.round < leader_round_wave_2) ||
                    // Add leader block which is the leader round of wave 2 (round == 6)
                    (block_ref.round == leader_round_wave_2 && block_ref.author == leader_schedule.elect_leader(leader_round_wave_2, 0)) {
                    Some(block.reference())
                } else {
                    None
                }
            })
            .collect::<Vec<BlockRef>>();

        let leaders = get_all_uncommitted_leader_blocks(
            dag_state.clone(),
            leader_schedule,
            leader_round_wave_2,
            wave_length,
            false,
            1,
        );

        last_commit_index += 1;
        let second_leader = leaders[1].clone();
        let expected_second_commit = TrustedCommit::new_for_test(
            last_commit_index,
            CommitDigest::MIN,
            0,
            second_leader.reference(),
            blocks.clone(),
        );

        let commit = linearizer.handle_commit(vec![second_leader.clone()]);
        assert_eq!(commit.len(), 1);

        let subdag = &commit[0];
        tracing::info!("{subdag:?}");
        assert_eq!(subdag.leader, second_leader.reference());
        assert_eq!(subdag.timestamp_ms, second_leader.timestamp_ms());
        assert_eq!(subdag.commit_index, expected_second_commit.index());

        // Using the same sorting as used in CommittedSubDag::sort
        blocks.sort_by(|a, b| a.round.cmp(&b.round).then_with(|| a.author.cmp(&b.author)));
        assert_eq!(
            subdag
                .blocks
                .clone()
                .into_iter()
                .map(|b| b.reference())
                .collect::<Vec<_>>(),
            blocks
        );
        for block in subdag.blocks.iter() {
            assert!(block.round() <= expected_second_commit.leader().round);
        }
    }
}
