// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashSet,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, BlockRef, BlockTimestampMs, Round, VerifiedBlock},
    commit::{Commit, CommitIndex},
    dag_state::DagState,
    storage::Store,
};

/// The output of consensus is an ordered list of [`CommittedSubDag`]. The application
/// can arbitrarily sort the blocks within each sub-dag (but using a deterministic algorithm).
#[derive(Clone)]
pub struct CommittedSubDag {
    /// A reference to the leader of the sub-dag
    pub leader: BlockRef,
    /// All the committed blocks that are part of this sub-dag
    pub blocks: Vec<VerifiedBlock>,
    /// The timestamp of the commit, obtained from the timestamp of the leader block.
    pub timestamp_ms: BlockTimestampMs,
    /// Index of the commit.
    /// First commit after genesis has a index of 1, then every next commit has a
    /// index incremented by 1.
    pub commit_index: CommitIndex,
}

#[allow(unused)]
impl CommittedSubDag {
    /// Create new (empty) sub-dag.
    pub fn new(
        leader: BlockRef,
        blocks: Vec<VerifiedBlock>,
        timestamp_ms: u64,
        commit_index: CommitIndex,
    ) -> Self {
        Self {
            leader,
            blocks,
            timestamp_ms,
            commit_index,
        }
    }

    // Used for recovery, which is why we need to get the data from block store.
    pub fn new_from_commit_data(commit_data: Commit, block_store: Arc<dyn Store>) -> Self {
        let mut leader_block_idx = None;
        let commit_blocks = block_store
            .read_blocks(&commit_data.blocks)
            .expect("We should have the block referenced in the commit data");
        let blocks = commit_blocks
            .into_iter()
            .enumerate()
            .map(|(idx, commit_block_opt)| {
                let commit_block = commit_block_opt
                    .expect("We should have the block referenced in the commit data");
                if commit_block.reference() == commit_data.leader {
                    leader_block_idx = Some(idx);
                }
                commit_block
            })
            .collect::<Vec<_>>();
        let leader_block_idx = leader_block_idx.expect("Leader block must be in the sub-dag");
        let leader_block_ref = blocks[leader_block_idx].reference();
        let timestamp_ms = blocks[leader_block_idx].timestamp_ms();
        CommittedSubDag::new(leader_block_ref, blocks, timestamp_ms, commit_data.index)
    }

    /// Sort the blocks of the sub-dag by round number then authority index. Any
    /// deterministic & stable algorithm works.
    pub fn sort(&mut self) {
        self.blocks.sort_by(|a, b| {
            a.round()
                .cmp(&b.round())
                .then_with(|| a.author().cmp(&b.author()))
        });
    }
}

impl Display for CommittedSubDag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CommittedSubDag(leader={}, index={}, blocks=[",
            self.leader, self.commit_index
        )?;
        for (idx, block) in self.blocks.iter().enumerate() {
            if idx > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", block.digest())?;
        }
        write!(f, "])")
    }
}

impl fmt::Debug for CommittedSubDag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{} (", self.leader, self.commit_index)?;
        for block in &self.blocks {
            write!(f, "{}, ", block.reference())?;
        }
        write!(f, ")")
    }
}

/// Expand a committed sequence of leader into a sequence of sub-dags.
#[allow(unused)]
#[derive(Clone)]
pub struct Linearizer {
    /// In memory block store representing the dag state
    dag_state: Arc<RwLock<DagState>>,
}

#[allow(unused)]
impl Linearizer {
    pub fn new(dag_state: Arc<RwLock<DagState>>) -> Self {
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
    // iterate over the sequence of committed leaders and produce a list of commit
    // data & committed sub-dags.
    pub fn handle_commit(
        &mut self,
        committed_leaders: Vec<VerifiedBlock>,
    ) -> Vec<(Commit, CommittedSubDag)> {
        let mut commits = vec![];

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
            commits.push((last_commit_data, sub_dag));
        }
        commits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use consensus_config::AuthorityIndex;

    use crate::{
        block::{BlockTimestampMs, TestBlock},
        commit::DEFAULT_WAVE_LENGTH,
        context::Context,
        leader_schedule::LeaderSchedule,
        storage::mem_store::MemStore,
    };

    #[test]
    fn test_handle_commit() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut linearizer = Linearizer::new(dag_state.clone());
        let leader_schedule = LeaderSchedule::new(context.clone());

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds: u32 = 10;
        let num_authorities: u32 = 4;
        let mut leaders = vec![];

        let (genesis_references, genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        dag_state.write().accept_blocks(genesis);

        let mut ancestors = genesis_references;
        for round in 1..=num_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                dag_state.write().accept_block(block.clone());
                new_ancestors.push(block.reference());

                if round % DEFAULT_WAVE_LENGTH == 0
                    && AuthorityIndex::new_for_test(author)
                        == leader_schedule.elect_leader(round, 0)
                {
                    leaders.push(block.clone());
                }
            }
            ancestors = new_ancestors;
        }

        let commits = linearizer.handle_commit(leaders.clone());
        for (idx, (_commit_data, subdag)) in commits.into_iter().enumerate() {
            tracing::info!("{subdag:?}");
            assert_eq!(subdag.leader, leaders[idx].reference());
            assert_eq!(subdag.timestamp_ms, leaders[idx].timestamp_ms());
            if idx == 0 {
                // First subdag includes the leader block plus all ancestor blocks
                // of the leader minus the genesis round blocks
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * (DEFAULT_WAVE_LENGTH - 1)) as usize + 1
                );
            } else {
                // Every subdag after will be missing the leader block from the previous
                // committed subdag
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * DEFAULT_WAVE_LENGTH) as usize
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
        let context = Arc::new(Context::new_for_test(4).0);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut linearizer = Linearizer::new(dag_state.clone());
        let wave_length = DEFAULT_WAVE_LENGTH;

        // Populate fully connected test blocks for round 0 ~ 3, authorities 0 ~ 3.
        let first_wave_rounds: u32 = wave_length;
        let num_authorities: u32 = 4;

        let mut blocks = Vec::new();
        let (genesis_references, _genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        blocks.append(&mut genesis_references.clone());

        let mut ancestors = genesis_references;
        let mut last_committed_rounds = vec![0; num_authorities as usize];
        for round in 1..=first_wave_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                new_ancestors.push(block.reference());
                blocks.push(block.reference());
                last_committed_rounds[block.author().value()] = block.round();

                // only write one block for the final round, which is the leader
                // of the committed subdag.
                if round == first_wave_rounds {
                    break;
                }
            }
            ancestors = new_ancestors;
        }

        let first_leader = *blocks.last().unwrap();
        let mut last_commit_index = 1;
        let first_commit_data = Commit {
            index: last_commit_index,
            leader: first_leader,
            blocks: blocks.clone(),
            last_committed_rounds,
        };
        dag_state.write().set_last_commit(first_commit_data);

        blocks.clear();
        let second_wave_rounds = first_wave_rounds + wave_length;
        for round in first_wave_rounds..=second_wave_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                // skip leader of last committed subdag as it was already written
                if round == first_leader.round && author == first_leader.author.value() as u32 {
                    continue;
                }
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                new_ancestors.push(block.reference());
                blocks.push(block.reference());
                dag_state.write().accept_block(block);

                // only write one block for the final round, which is the leader
                // of the next committed subdag.
                if round == second_wave_rounds {
                    break;
                }
            }
            ancestors = new_ancestors;
        }

        last_commit_index += 1;
        let second_leader = *blocks.last().unwrap();
        let expected_second_commit_data = Commit {
            index: last_commit_index,
            leader: second_leader,
            blocks: blocks.clone(),
            last_committed_rounds: vec![],
        };

        let second_leader_block = dag_state
            .read()
            .get_uncommitted_block(&second_leader)
            .unwrap();
        let commit = linearizer.handle_commit(vec![second_leader_block.clone()]);
        assert_eq!(commit.len(), 1);

        let (_commit_data, subdag) = &commit[0];
        tracing::info!("{subdag:?}");
        assert_eq!(subdag.leader, second_leader);
        assert_eq!(subdag.timestamp_ms, second_leader_block.timestamp_ms());
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

    #[test]
    fn test_new_subdag_from_commit_data() {
        let store = Arc::new(MemStore::new());
        let context = Arc::new(Context::new_for_test(4).0);
        let wave_length = DEFAULT_WAVE_LENGTH;

        // Populate fully connected test blocks for round 0 ~ 3, authorities 0 ~ 3.
        let first_wave_rounds: u32 = wave_length;
        let num_authorities: u32 = 4;

        let mut blocks = Vec::new();
        let (genesis_references, genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        store.write(genesis, vec![]).unwrap();
        blocks.append(&mut genesis_references.clone());

        let mut ancestors = genesis_references;
        let mut leader = None;
        for round in 1..=first_wave_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                store.write(vec![block.clone()], vec![]).unwrap();
                new_ancestors.push(block.reference());
                blocks.push(block.reference());

                // only write one block for the final round, which is the leader
                // of the committed subdag.
                if round == first_wave_rounds {
                    leader = Some(block.clone());
                    break;
                }
            }
            ancestors = new_ancestors;
        }

        let leader_block = leader.unwrap();
        let leader_ref = leader_block.reference();
        let commit_index = 1;
        let commit_data = Commit {
            index: commit_index,
            leader: leader_ref,
            blocks: blocks.clone(),
            last_committed_rounds: vec![],
        };

        let subdag = CommittedSubDag::new_from_commit_data(commit_data, store.clone());
        assert_eq!(subdag.leader, leader_ref);
        assert_eq!(subdag.timestamp_ms, leader_block.timestamp_ms());
        assert_eq!(
            subdag.blocks.len(),
            (num_authorities * wave_length) as usize + 1
        );
        assert_eq!(subdag.commit_index, commit_index);
    }
}
