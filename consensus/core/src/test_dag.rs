// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, BlockTimestampMs, Round};
use parking_lot::RwLock;
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::{
    block::{TestBlock, VerifiedBlock, genesis_blocks},
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
        None => genesis_blocks(context.as_ref())
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

/// Controls Byzantine equivocations injected while generating a randomized DAG.
///
/// At most `floor((N - 1) / 3)` distinct authorities can equivocate in one round,
/// regardless of `max_equivocators`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RandomDagEquivocationConfig {
    /// Percentage of rounds in which Byzantine authorities equivocate.
    pub equivocation_rate: u8,
    /// Upper bound on the number of distinct equivocating authorities per round.
    pub max_equivocators: usize,
    /// Number of additional conflicting blocks each selected authority produces.
    pub equivocations_per_authority: usize,
}

pub(crate) fn create_random_dag(
    seed: u64,
    include_leader_percentage: u64,
    num_rounds: Round,
    context: Arc<Context>,
    equivocation_config: RandomDagEquivocationConfig,
) -> DagBuilder {
    assert!(
        (0..=100).contains(&include_leader_percentage),
        "include_leader_percentage must be in the range 0..100"
    );
    assert!(
        equivocation_config.equivocation_rate <= 100,
        "equivocation_rate must be in the range 0..=100"
    );

    let mut rng = StdRng::seed_from_u64(seed);
    let mut dag_builder = DagBuilder::new(context);
    let max_safe_equivocators = (dag_builder.context.committee.size() - 1) / 3;
    let max_equivocators = equivocation_config
        .max_equivocators
        .min(max_safe_equivocators);
    let mut byzantine_authorities = dag_builder
        .context
        .committee
        .authorities()
        .map(|(authority, _)| authority)
        .collect::<Vec<_>>();
    byzantine_authorities.shuffle(&mut rng);
    byzantine_authorities.truncate(max_equivocators);

    for r in 1..=num_rounds {
        let random_num = rng.gen_range(0..100);
        let include_leader = random_num <= include_leader_percentage;
        let min_ancestor_links_seed = rng.r#gen();

        let should_equivocate = equivocation_config.equivocations_per_authority > 0
            && max_equivocators > 0
            && rng.gen_range(0..100) < equivocation_config.equivocation_rate;
        if should_equivocate {
            let num_equivocators = rng.gen_range(1..=max_equivocators);
            let mut authorities = byzantine_authorities.clone();
            authorities.shuffle(&mut rng);

            dag_builder
                .layer(r)
                .authorities(authorities.into_iter().take(num_equivocators).collect())
                .equivocate(equivocation_config.equivocations_per_authority)
                .min_ancestor_links(include_leader, Some(min_ancestor_links_seed));
        } else {
            dag_builder
                .layer(r)
                .min_ancestor_links(include_leader, Some(min_ancestor_links_seed));
        }
    }

    dag_builder
}
