// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    block::{genesis_blocks, BlockRef, BlockTimestampMs, Round, TestBlock, VerifiedBlock},
    context::Context,
    dag_state::DagState,
    test_dag_builder::DagBuilder,
};

// todo: remove this once tests have been refactored to use DagBuilder/DagParser

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
        None => genesis_blocks(context.clone())
            .iter()
            .map(|x| x.reference())
            .collect::<Vec<_>>(),
    };

    let num_authorities = context.committee.size();
    let starting_round = ancestors.first().unwrap().round + 1;
    for round in starting_round..=stop {
        let (references, blocks): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|authority| {
                let author_idx = authority.0.value() as u32;
                // Test the case where a block from round R+1 has smaller timestamp than a block from round R.
                let ts = round as BlockTimestampMs / 2 * num_authorities as BlockTimestampMs
                    + author_idx as BlockTimestampMs;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author_idx)
                        .set_timestamp_ms(ts)
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

pub(crate) fn create_random_dag(
    seed: u64,
    include_leader_percentage: u64,
    num_rounds: Round,
    context: Arc<Context>,
) -> DagBuilder {
    assert!(
        (0..=100).contains(&include_leader_percentage),
        "include_leader_percentage must be in the range 0..100"
    );

    let mut rng = StdRng::seed_from_u64(seed);
    let mut dag_builder = DagBuilder::new(context);

    for r in 1..=num_rounds {
        let random_num = rng.gen_range(0..100);
        let include_leader = random_num <= include_leader_percentage;
        dag_builder
            .layer(r)
            .min_ancestor_links(include_leader, Some(random_num));
    }

    dag_builder
}
