// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display, Formatter};

use consensus_config::AuthorityIndex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    block::{BlockAPI, BlockRef, BlockTimestampMs, Round, Slot, VerifiedBlock},
    storage::Store,
};

/// Default wave length for all committers. A longer wave length increases the
/// chance of committing the leader under asynchrony at the cost of latency in
/// the common case.
pub(crate) const DEFAULT_WAVE_LENGTH: Round = MINIMUM_WAVE_LENGTH;

/// We need at least one leader round, one voting round, and one decision round.
pub(crate) const MINIMUM_WAVE_LENGTH: Round = 3;

/// The consensus protocol operates in 'waves'. Each wave is composed of a leader
/// round, at least one voting round, and one decision round.
#[allow(unused)]
pub(crate) type WaveNumber = u32;

/// Index of the commit.
pub(crate) type CommitIndex = u64;

/// Specifies one consensus commit.
/// It is stored on disk, so it does not contain blocks which are stored individually.
#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Commit {
    /// Index of the commit.
    /// First commit after genesis has an index of 1, then every next commit has an index incremented by 1.
    pub index: CommitIndex,
    /// A reference to the the commit leader.
    pub leader: BlockRef,
    /// Refs to committed blocks, in the commit order.
    pub blocks: Vec<BlockRef>,
    /// Last committed round per authority.
    pub last_committed_rounds: Vec<Round>,
}

/// The output of consensus is an ordered list of [`CommittedSubDag`]. The application
/// can arbitrarily sort the blocks within each sub-dag (but using a deterministic algorithm).
#[derive(Clone, PartialEq)]
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

// Recovers the full CommittedSubDag from block store, based on Commit.
pub fn load_committed_subdag_from_store(
    block_store: &dyn Store,
    commit_data: Commit,
) -> CommittedSubDag {
    let mut leader_block_idx = None;
    let commit_blocks = block_store
        .read_blocks(&commit_data.blocks)
        .expect("We should have the block referenced in the commit data");
    let blocks = commit_blocks
        .into_iter()
        .enumerate()
        .map(|(idx, commit_block_opt)| {
            let commit_block =
                commit_block_opt.expect("We should have the block referenced in the commit data");
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

#[allow(unused)]
pub struct CommitConsumer {
    // A channel to send the committed sub dags through
    pub sender: UnboundedSender<CommittedSubDag>,
    // The last commit index that the consumer has processed. This is useful for
    // crash/recovery so mysticeti can replay the commits from `last_processed_index + 1`.
    pub last_processed_index: u64,
}

#[allow(unused)]
impl CommitConsumer {
    pub fn new(sender: UnboundedSender<CommittedSubDag>, last_processed_index: u64) -> Self {
        Self {
            sender,
            last_processed_index,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(unused)]
pub(crate) enum Decision {
    Direct,
    Indirect,
}

/// The status of every leader output by the committers. While the core only cares
/// about committed leaders, providing a richer status allows for easier debugging,
/// testing, and composition with advanced commit strategies.
#[allow(unused)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LeaderStatus {
    Commit(VerifiedBlock),
    Skip(Slot),
    Undecided(Slot),
}

#[allow(unused)]
impl LeaderStatus {
    pub(crate) fn round(&self) -> Round {
        match self {
            Self::Commit(block) => block.round(),
            Self::Skip(leader) => leader.round,
            Self::Undecided(leader) => leader.round,
        }
    }

    pub(crate) fn authority(&self) -> AuthorityIndex {
        match self {
            Self::Commit(block) => block.author(),
            Self::Skip(leader) => leader.authority,
            Self::Undecided(leader) => leader.authority,
        }
    }

    pub(crate) fn is_decided(&self) -> bool {
        match self {
            Self::Commit(_) => true,
            Self::Skip(_) => true,
            Self::Undecided(_) => false,
        }
    }

    // Only should be called when the leader status is decided (Commit/Skip)
    pub fn get_decided_slot(&self) -> Slot {
        match self {
            Self::Commit(block) => block.reference().into(),
            Self::Skip(leader) => *leader,
            Self::Undecided(..) => panic!("Decided block is either Commit or Skip"),
        }
    }

    // Only should be called when the leader status is decided (Commit/Skip)
    pub fn into_committed_block(self) -> Option<VerifiedBlock> {
        match self {
            Self::Commit(block) => Some(block),
            Self::Skip(leader) => None,
            Self::Undecided(..) => panic!("Decided block is either Commit or Skip"),
        }
    }
}

impl Display for LeaderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(block) => write!(f, "Commit({})", block.reference()),
            Self::Skip(leader) => write!(f, "Skip({leader})"),
            Self::Undecided(leader) => write!(f, "Undecided({leader})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{block::TestBlock, context::Context, storage::mem_store::MemStore};

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

        let subdag = load_committed_subdag_from_store(store.as_ref(), commit_data);
        assert_eq!(subdag.leader, leader_ref);
        assert_eq!(subdag.timestamp_ms, leader_block.timestamp_ms());
        assert_eq!(
            subdag.blocks.len(),
            (num_authorities * wave_length) as usize + 1
        );
        assert_eq!(subdag.commit_index, commit_index);
    }
}
