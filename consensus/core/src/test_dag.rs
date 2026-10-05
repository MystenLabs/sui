// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
/// A fixed set of at most `floor((N - 1) / 3)` distinct authorities can
/// equivocate throughout the DAG, regardless of `max_equivocators`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RandomDagEquivocationConfig {
    /// Percentage of rounds in which authorities from the fixed Byzantine set equivocate.
    pub equivocation_rate: u8,
    /// Upper bound on the size of the fixed Byzantine set.
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
    // Randomized consensus tests use equal-stake test committees.
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
        let include_leader = random_num < include_leader_percentage;
        let min_ancestor_links_seed = rng.r#gen();
        let should_equivocate_this_round =
            rng.gen_range(0..100) < equivocation_config.equivocation_rate;

        let should_equivocate = equivocation_config.equivocations_per_authority > 0
            && max_equivocators > 0
            && should_equivocate_this_round;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockAPI;

    #[test]
    fn random_dag_equivocations_use_a_fixed_safe_authority_set() {
        let num_authorities = 7;
        let num_rounds = 100;
        let context = Arc::new(Context::new_for_test(num_authorities).0);
        let dag_builder = create_random_dag(
            7,
            100,
            num_rounds,
            context.clone(),
            RandomDagEquivocationConfig {
                equivocation_rate: 100,
                max_equivocators: usize::MAX,
                equivocations_per_authority: 2,
            },
        );
        let max_safe_equivocators = (num_authorities - 1) / 3;
        let max_parent_links = context.committee.quorum_threshold() as usize + 1;
        let mut all_equivocators = BTreeSet::new();
        let mut saw_equivocation = false;
        let mut saw_max_equivocators = false;

        for round in 1..=num_rounds {
            let blocks_per_author = dag_builder
                .blocks
                .values()
                .filter(|block| block.round() == round)
                .fold(BTreeMap::new(), |mut counts, block| {
                    *counts.entry(block.author()).or_insert(0) += 1;
                    counts
                });
            let equivocators = blocks_per_author
                .values()
                .filter(|&&blocks| blocks > 1)
                .count();
            all_equivocators.extend(
                blocks_per_author
                    .iter()
                    .filter_map(|(&author, &blocks)| (blocks > 1).then_some(author)),
            );
            assert!(
                equivocators <= max_safe_equivocators,
                "round {round} has {equivocators} equivocating authorities"
            );
            saw_equivocation |= equivocators > 0;
            saw_max_equivocators |= equivocators == max_safe_equivocators;
        }

        assert!(saw_equivocation);
        assert!(saw_max_equivocators);
        assert!(all_equivocators.len() <= max_safe_equivocators);
        assert!(dag_builder.blocks.values().all(|block| {
            let mut ancestor_authors = BTreeSet::new();
            block
                .ancestors()
                .iter()
                .all(|ancestor| ancestor_authors.insert(ancestor.author))
        }));
        assert!(dag_builder.blocks.values().all(|block| {
            block
                .ancestors()
                .first()
                .is_some_and(|ancestor| ancestor.author == block.author())
        }));
        assert!(
            dag_builder
                .blocks
                .values()
                .all(|block| block.ancestors().len() <= max_parent_links)
        );
    }
}
