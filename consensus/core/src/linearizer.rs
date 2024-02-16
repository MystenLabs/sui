// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, Round, VerifiedBlock},
    commit::{Commit, CommitIndex, CommittedSubDag},
    dag_state::DagState,
};

/// Expand a committed sequence of leader into a sequence of sub-dags.
#[allow(unused)]
#[derive(Clone)]
pub(crate) struct Linearizer {
    /// In memory block store representing the dag state
    dag_state: Arc<RwLock<DagState>>,
}

#[allow(unused)]
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
        last_committed_rounds: Vec<Round>,
    ) -> CommittedSubDag {
        let mut to_commit = Vec::new();
        let mut committed = HashSet::new();

        let timestamp_ms = leader_block.timestamp_ms();
        let leader_block_ref = leader_block.reference();
        let mut buffer = vec![leader_block];
        assert!(committed.insert(leader_block_ref));

        let dag_state = self.dag_state.read();
        while let Some(x) = buffer.pop() {
            to_commit.push(x.clone());

            let ancestors: Vec<VerifiedBlock> = dag_state
                .get_uncommitted_blocks(
                    x.ancestors()
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
        let mut commits = vec![];
        let mut committed_blocks = vec![];

        for leader_block in committed_leaders {
            // Grab latest commit state from dag state
            let dag_state = self.dag_state.read();
            let mut last_commit_index = dag_state.last_commit_index();
            let mut last_committed_rounds = dag_state.last_committed_rounds();
            drop(dag_state);

            // Collect the sub-dag generated using each of these leaders.
            let mut sub_dag = self.collect_sub_dag(
                leader_block,
                last_commit_index,
                last_committed_rounds.clone(),
            );

            // [Optional] sort the sub-dag using a deterministic algorithm.
            sub_dag.sort();

            // Update last commit in dag state
            let last_commit_data = Commit {
                index: sub_dag.commit_index,
                leader: sub_dag.leader,
                blocks: sub_dag
                    .blocks
                    .iter()
                    .map(|block| {
                        let block_ref = block.reference();
                        last_committed_rounds[block_ref.author.value()] = block_ref.round;
                        block_ref
                    })
                    .collect::<Vec<_>>(),
                last_committed_rounds,
            };
            self.dag_state
                .write()
                .set_last_commit(last_commit_data.clone());
            commits.push(last_commit_data);
            committed_blocks.extend(sub_dag.blocks.clone());
            committed_sub_dags.push(sub_dag);
        }
        // TODO: Revisit this after refactor of dag state
        if !commits.is_empty() {
            self.dag_state
                .write()
                .write_commits(commits, committed_blocks);
        }
        committed_sub_dags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commit::DEFAULT_WAVE_LENGTH,
        context::Context,
        leader_schedule::LeaderSchedule,
        storage::mem_store::MemStore,
        test_dag::{build_dag, get_all_leader_blocks},
    };

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
        let leader_schedule = LeaderSchedule::new(context.clone());

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds: u32 = 10;
        build_dag(context.clone(), dag_state.clone(), None, num_rounds);
        let leaders = get_all_leader_blocks(
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
            assert_eq!(subdag.commit_index, idx as u64 + 1);
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
        let leader_schedule = LeaderSchedule::new(context.clone());
        let mut linearizer = Linearizer::new(dag_state.clone());
        let wave_length = DEFAULT_WAVE_LENGTH;

        let mut blocks = vec![];
        let mut ancestors = None;
        let leader_round_wave_1 = 3;

        // Build dag layers for rounds 0 ~ 2 and maintain list of blocks to be included
        // in the subdag of the leader of wave 1.
        for n in 0..leader_round_wave_1 {
            ancestors = Some(build_dag(context.clone(), dag_state.clone(), ancestors, n));
            blocks.extend(ancestors.clone().unwrap());
        }

        // Build dag layer for round 3 which is the leader round of wave 1
        ancestors = Some(build_dag(
            context.clone(),
            dag_state.clone(),
            ancestors,
            leader_round_wave_1,
        ));

        let leaders = get_all_leader_blocks(
            dag_state.clone(),
            leader_schedule.clone(),
            leader_round_wave_1,
            wave_length,
            false,
            1,
        );

        // Add leader block to first committed subdag blocks
        blocks.push(leaders[0].reference());

        let mut last_committed_rounds = vec![0; num_authorities];
        for block in blocks.iter() {
            let last_committed_round = last_committed_rounds[block.author];
            last_committed_rounds[block.author] = std::cmp::max(last_committed_round, block.round);
        }

        let first_leader = leaders[0].clone();
        let mut last_commit_index = 1;
        let first_commit_data = Commit {
            index: last_commit_index,
            leader: first_leader.reference(),
            blocks: blocks.clone(),
            last_committed_rounds,
        };
        dag_state.write().set_last_commit(first_commit_data);

        blocks.clear();
        let leader_round_wave_2 = leader_round_wave_1 + wave_length;

        // Add all nonleader blocks from round 3 to the blocks which will be part
        // of the second committed subdag
        blocks.extend(
            ancestors
                .clone()
                .unwrap()
                .into_iter()
                .filter(|block_ref| block_ref.author != first_leader.author())
                .collect::<Vec<_>>(),
        );

        // Build dag layers for rounds 4 ~ 5 and maintain list of blocks to be included
        // in the subdag of the leader of wave 1.
        for n in leader_round_wave_1 + 1..leader_round_wave_2 {
            ancestors = Some(build_dag(context.clone(), dag_state.clone(), ancestors, n));
            blocks.extend(ancestors.clone().unwrap());
        }

        // Build dag layer for round 6 which is the leader round of wave 2
        build_dag(
            context.clone(),
            dag_state.clone(),
            ancestors,
            leader_round_wave_2,
        );

        let leaders = get_all_leader_blocks(
            dag_state.clone(),
            leader_schedule,
            leader_round_wave_2,
            wave_length,
            false,
            1,
        );

        // Add leader block to second committed subdag blocks
        blocks.push(leaders[1].reference());

        last_commit_index += 1;
        let second_leader = leaders[1].clone();
        let expected_second_commit_data = Commit {
            index: last_commit_index,
            leader: second_leader.reference(),
            blocks: blocks.clone(),
            last_committed_rounds: vec![],
        };

        let commit = linearizer.handle_commit(vec![second_leader.clone()]);
        assert_eq!(commit.len(), 1);

        let subdag = &commit[0];
        tracing::info!("{subdag:?}");
        assert_eq!(subdag.leader, second_leader.reference());
        assert_eq!(subdag.timestamp_ms, second_leader.timestamp_ms());
        assert_eq!(subdag.commit_index, expected_second_commit_data.index);

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
            assert!(block.round() <= expected_second_commit_data.leader.round);
        }
    }
}
