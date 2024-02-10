// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    block::{Block, BlockRef, BlockTimestampMs, Round, Slot, TestBlock, VerifiedBlock},
    context::Context,
    dag_state::DagState,
    leader_schedule::LeaderSchedule,
};

/// Build a fully interconnected dag up to the specified round. This function
/// starts building the dag from the specified [`start`] parameter or from
/// genesis if none are specified up to and including the specified round [`stop`]
/// parameter.
pub(crate) fn build_dag(
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    start: Option<Vec<BlockRef>>,
    stop: Round,
) -> Vec<BlockRef> {
    let mut ancestors = match start {
        Some(start) => {
            assert!(!start.is_empty());
            assert_eq!(
                start.iter().map(|x| x.round).max(),
                start.iter().map(|x| x.round).min()
            );
            start
        }
        None => {
            let (_my_genesis_block, genesis) = Block::genesis(context.clone());
            let references = genesis.iter().map(|x| x.reference()).collect::<Vec<_>>();
            dag_state.write().accept_blocks(genesis);

            references
        }
    };

    let starting_round = ancestors.first().unwrap().round + 1;
    for round in starting_round..=stop {
        let (references, blocks): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|authority| {
                let author_idx = authority.0.value() as u32;
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author_idx)
                        .set_timestamp_ms(base_ts + (author_idx + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );

                (block.reference(), block)
            })
            .unzip();
        dag_state.write().accept_blocks(blocks);
        ancestors = references;
    }

    ancestors
}

// TODO: Add layer_round as input parameter so ancestors can be from any round.
pub(crate) fn build_dag_layer(
    // A list of (authority, parents) pairs. For each authority, we add a block
    // linking to the specified parents.
    connections: Vec<(AuthorityIndex, Vec<BlockRef>)>,
    dag_state: Arc<RwLock<DagState>>,
) -> Vec<BlockRef> {
    let mut references = Vec::new();
    for (authority, ancestors) in connections {
        let round = ancestors.first().unwrap().round + 1;
        let author = authority.value() as u32;
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(round, author)
                .set_ancestors(ancestors)
                .build(),
        );
        references.push(block.reference());
        dag_state.write().accept_block(block);
    }
    references
}

// TODO: confirm pipelined & multi-leader cases work properly
pub(crate) fn get_all_leader_blocks(
    dag_state: Arc<RwLock<DagState>>,
    leader_schedule: LeaderSchedule,
    num_rounds: u32,
    wave_length: u32,
    pipelined: bool,
    num_leaders: u32,
) -> Vec<VerifiedBlock> {
    let mut blocks = Vec::new();
    for round in 1..=num_rounds {
        for leader_offset in 0..num_leaders {
            if pipelined || round % wave_length == 0 {
                let slot = Slot::new(round, leader_schedule.elect_leader(round, leader_offset));
                let uncommitted_blocks = dag_state.read().get_uncommitted_blocks_at_slot(slot);
                blocks.extend(uncommitted_blocks);
            }
        }
    }
    blocks
}
