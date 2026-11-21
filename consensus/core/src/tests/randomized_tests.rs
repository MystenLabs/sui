// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, future::Future, panic::AssertUnwindSafe};

use futures::FutureExt as _;
use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

use crate::{
    block::Slot,
    commit::DEFAULT_WAVE_LENGTH,
    commit_test_fixture::{CommitTestFixture, RandomDag, assert_commit_sequences_match},
    test_dag::{RandomDagEquivocationConfig, create_random_dag},
};

const NUM_RUNS: u32 = 100;
// The equivocation test covers 12 fixed DAG configurations, so a smaller number
// of deterministic delivery permutations keeps the regular CI suite responsive.
const EQUIVOCATION_DELIVERY_PERMUTATIONS: u32 = 10;
const MAX_STEP: u32 = 3;
// A shorter DAG has no commits to compare, and the tests subtract from the number of rounds.
const MIN_SHRUNK_ROUNDS: u32 = 3;

/// The parameters of a randomized DAG test that a failure is shrunk over.
///
/// The DAG for a given seed is generated round by round from one random number generator, so the
/// DAG of `n` rounds is the first `n` rounds of any longer DAG with the same seed. Delivery
/// orders are derived from the seeds as well, so a smaller failing case is a reproducible,
/// shorter version of the original failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DagTestParams {
    seed: u64,
    include_leader_percentage: u64,
    num_rounds: u32,
    /// Zero means that no authority equivocates.
    equivocations_per_authority: usize,
}

impl DagTestParams {
    fn equivocation_config(&self) -> RandomDagEquivocationConfig {
        if self.equivocations_per_authority == 0 {
            return RandomDagEquivocationConfig::default();
        }
        RandomDagEquivocationConfig {
            equivocation_rate: 100,
            // Deliberately above F: the generator must cap this at F.
            max_equivocators: usize::MAX,
            equivocations_per_authority: self.equivocations_per_authority,
        }
    }

    /// Strictly smaller parameters to try, the smallest first. Authorities keep at least one
    /// equivocation when the test starts with some, because the test then checks a property
    /// that needs it.
    fn shrink_candidates(&self, original: &Self) -> Vec<Self> {
        let rounds = self.num_rounds;
        let mut smaller_rounds = vec![rounds / 2, rounds - rounds / 4, rounds.saturating_sub(1)];
        smaller_rounds.retain(|&candidate| (MIN_SHRUNK_ROUNDS..rounds).contains(&candidate));
        smaller_rounds.dedup();

        let min_equivocations = original.equivocations_per_authority.min(1);
        let fewer_equivocations = (self.equivocations_per_authority > min_equivocations)
            .then(|| self.equivocations_per_authority - 1);

        smaller_rounds
            .into_iter()
            .map(|num_rounds| Self {
                num_rounds,
                ..*self
            })
            .chain(fewer_equivocations.map(|equivocations_per_authority| Self {
                equivocations_per_authority,
                ..*self
            }))
            .collect()
    }
}

/// Runs `check` and returns the message of its panic, if any.
async fn failure_of<F, Fut>(check: &F, params: DagTestParams) -> Option<String>
where
    F: Fn(DagTestParams) -> Fut,
    Fut: Future<Output = ()>,
{
    let payload = AssertUnwindSafe(check(params)).catch_unwind().await.err()?;
    let message = payload.downcast_ref::<String>().cloned().or_else(|| {
        payload
            .downcast_ref::<&str>()
            .map(|message| message.to_string())
    });
    Some(message.unwrap_or_else(|| "panic without a message".to_string()))
}

/// The words of a failure message before its first digit. A shorter DAG can fail a different
/// check, such as one that needs at least a few commits, so a smaller case only counts when its
/// failure has the same words as the original ("More than one certified leader at wave", say).
fn failure_kind(failure: &str) -> &str {
    match failure
        .split(|character: char| character.is_ascii_digit())
        .next()
    {
        Some(words) if !words.is_empty() => words,
        _ => failure,
    }
}

/// Greedily replaces `params` with a smaller case for which `failure_of` still reports a
/// failure, until no smaller case fails. Returns the smallest case and its failure.
async fn shrink<F, Fut>(
    params: DagTestParams,
    failure: String,
    mut failure_of: F,
) -> (DagTestParams, String)
where
    F: FnMut(DagTestParams) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    let original = params;
    let (mut smallest, mut smallest_failure) = (params, failure);
    'shrinking: loop {
        for candidate in smallest.shrink_candidates(&original) {
            if let Some(failure) = failure_of(candidate).await {
                (smallest, smallest_failure) = (candidate, failure);
                continue 'shrinking;
            }
        }
        return (smallest, smallest_failure);
    }
}

/// Runs `check` with `params`. If it fails, searches for a smaller case that fails in the same
/// way and fails the test with the smallest one found, so that a failing seed is easier to debug.
async fn check_with_shrinking<F, Fut>(params: DagTestParams, check: F)
where
    F: Fn(DagTestParams) -> Fut,
    Fut: Future<Output = ()>,
{
    let Some(failure) = failure_of(&check, params).await else {
        return;
    };
    let kind = failure_kind(&failure).to_string();
    let (smallest, smallest_failure) = shrink(params, failure.clone(), |candidate| {
        let (check, kind) = (&check, &kind);
        async move {
            failure_of(check, candidate)
                .await
                .filter(|failure| failure_kind(failure) == kind)
        }
    })
    .await;
    panic!(
        "Randomized DAG test failed with {params:?}: {failure}\n\
         Smallest case that fails in the same way: {smallest:?}: {smallest_failure}\n\
         DAG_TEST_SEED and DAG_TEST_ROUNDS override the seed and the number of rounds of the \
         tests that read them."
    );
}

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round.
///
/// Should result in a direct commit for every round.
#[tokio::test]
async fn test_randomized_dag_all_direct_commit() {
    let params = DagTestParams {
        seed: random_test_seed(),
        include_leader_percentage: 100,
        num_rounds: random_test_rounds(1000),
        equivocations_per_authority: 0,
    };
    check_with_shrinking(params, check_all_direct_commit).await;
}

async fn check_all_direct_commit(params: DagTestParams) {
    let DagTestParams {
        seed,
        include_leader_percentage,
        num_rounds,
        ..
    } = params;
    let num_authorities = 7;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
    let dag_builder = create_random_dag(
        seed,
        include_leader_percentage,
        num_rounds,
        context.clone(),
        params.equivocation_config(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with {num_authorities} authorities & {num_rounds} rounds in the DAG with seed {seed}..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());
        fixture.add_blocks(all_blocks.clone());

        let last_decided = Slot::new_for_test(0, 0);
        let (finalized_commits, _) = fixture.try_commit(last_decided).await;
        commit_sequences.push(finalized_commits);
    }

    let last_commit_sequence = assert_commit_sequences_match(commit_sequences);
    assert_eq!(last_commit_sequence.len(), (num_rounds - 2) as usize);
}

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round 50% of the time.
///
/// Blocks are delivered via RandomDagIterator in a constrained random order based on
/// quorum rounds, simulating realistic block arrival patterns. After each accepted
/// block we will try_commit() and if there is a committed sequence we will update
/// last_decided and continue. We do this from the perspective of different
/// authorities who receive the blocks in different orders and ensure the resulting
/// sequence is the same for all authorities. The resulting sequence will include
/// Commit & Skip decisions and potentially will stop before coming to a decision
/// on all waves as we may have an Undecided leader somewhere early in the sequence.
///
/// Additionally, this test processes commits through the Linearizer and CommitFinalizer
/// incrementally after each try_commit() call, similar to the production flow.
#[tokio::test]
async fn test_randomized_dag_and_decision_sequence() {
    let params = DagTestParams {
        seed: random_test_seed(),
        include_leader_percentage: 50,
        num_rounds: random_test_rounds(1000),
        equivocations_per_authority: 0,
    };
    check_with_shrinking(params, check_decision_sequence).await;
}

async fn check_decision_sequence(params: DagTestParams) {
    let DagTestParams {
        seed,
        include_leader_percentage,
        num_rounds,
        ..
    } = params;
    let mut rng = StdRng::seed_from_u64(seed);
    let num_authorities = 7;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
    let dag_builder = create_random_dag(
        seed,
        include_leader_percentage,
        num_rounds,
        context.clone(),
        params.equivocation_config(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Create RandomDag from existing blocks for using RandomDagIterator
    let dag = RandomDag::from_blocks(context.clone(), all_blocks);

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with {num_authorities} authorities & {num_rounds} rounds in the DAG with seed {seed}..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());
        let mut finalized_commits = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);

        // Use RandomDagIterator to deliver blocks in constrained random order
        for block in dag.random_iter(&mut rng, MAX_STEP) {
            fixture.try_accept_blocks(vec![block]);

            let (finalized, new_last_decided) = fixture.try_commit(last_decided).await;
            finalized_commits.extend(finalized);
            last_decided = new_last_decided;
        }

        assert!(fixture.has_no_suspended_blocks());
        commit_sequences.push(finalized_commits);
    }

    assert_commit_sequences_match(commit_sequences);
}

/// Randomized delivery must not make honest authorities commit divergent sequences
/// when up to F Byzantine authorities equivocate in a round.
#[tokio::test]
async fn test_randomized_dag_with_equivocations() {
    let num_rounds = 100;
    // Fixed base seeds make failures reproducible while exercising distinct choices
    // of equivocating authorities and minimal-parent links.
    for (leader_config_index, include_leader_percentage) in [100u64, 50].into_iter().enumerate() {
        for (equivocation_config_index, equivocations_per_authority) in
            [1usize, 2].into_iter().enumerate()
        {
            // Derive a per-configuration seed so the 4 (include_leader_percentage,
            // equivocations_per_authority) configurations under one base seed select
            // different (but still deterministic and reproducible) Byzantine
            // authority sets, instead of all 4 reusing the same shuffle draw from
            // the base seed. StdRng::seed_from_u64 hashes its input, so nearby inputs
            // like these produce decorrelated states.
            let config_index = (leader_config_index * 2 + equivocation_config_index) as u64;

            for seed in [7u64, 42, 2026] {
                let params = DagTestParams {
                    seed: seed.wrapping_mul(4).wrapping_add(config_index),
                    include_leader_percentage,
                    num_rounds,
                    equivocations_per_authority,
                };
                check_with_shrinking(params, |params| check_equivocations(params, seed)).await;
            }
        }
    }
}

async fn check_equivocations(params: DagTestParams, base_seed: u64) {
    let DagTestParams {
        seed: dag_seed,
        include_leader_percentage,
        num_rounds,
        ..
    } = params;
    let num_authorities = 7;
    let max_safe_equivocators = (num_authorities - 1) / 3;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
    let dag_builder = create_random_dag(
        dag_seed,
        include_leader_percentage,
        num_rounds,
        context.clone(),
        params.equivocation_config(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
    let dag = RandomDag::from_blocks(context.clone(), all_blocks);
    let mut delivery_rng = StdRng::seed_from_u64(base_seed);
    let mut commit_sequences = vec![];
    let mut skipped_leaders = 0;

    for _ in 0..EQUIVOCATION_DELIVERY_PERMUTATIONS {
        let mut fixture = CommitTestFixture::new(context.clone());
        let mut finalized_commits = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);

        for block in dag.random_iter(&mut delivery_rng, MAX_STEP) {
            fixture.try_accept_blocks(vec![block]);
            let (finalized, new_last_decided, skipped) =
                fixture.try_commit_with_skip_count(last_decided).await;
            finalized_commits.extend(finalized);
            last_decided = new_last_decided;
            skipped_leaders += skipped;
        }

        assert!(fixture.has_no_suspended_blocks());
        commit_sequences.push(finalized_commits);
    }

    let commits = assert_commit_sequences_match(commit_sequences);
    if include_leader_percentage == 100 {
        // With every voter linking the leader, a skip can only come from
        // votes splitting across an equivocating leader's forks. At lower
        // include_leader_percentage, honest leaders can also be skipped by
        // blame alone, so skipped_leaders > 0 no longer implies an
        // equivocating leader was hit.
        let equivocations_per_authority = params.equivocations_per_authority;
        assert!(
            skipped_leaders > 0,
            "equivocations did not cause any leader to be skipped for \
             base_seed={base_seed} dag_seed={dag_seed} \
             include_leader_percentage={include_leader_percentage} \
             equivocations_per_authority={equivocations_per_authority}"
        );
        let last_commit_round = commits
            .last()
            .expect("honest quorum did not make progress")
            .leader
            .round;
        // BaseCommitter::try_indirect_decide requires an anchor at least
        // one wave length above the leader slot it resolves, and stops at
        // the first undecided anchor. Each of the up to
        // `max_safe_equivocators` Byzantine leaders can leave its own slot
        // undecided until a further DEFAULT_WAVE_LENGTH rounds of blocks
        // arrive, so stacking all of them one wave apart near the tip
        // pushes the last leader round guaranteed to be decided down to:
        let min_decidable_round =
            num_rounds.saturating_sub(2 + DEFAULT_WAVE_LENGTH * max_safe_equivocators as u32);
        assert!(
            last_commit_round >= min_decidable_round,
            "honest quorum stopped making progress at round {last_commit_round} \
             (expected >= {min_decidable_round}) for base_seed={base_seed} \
             dag_seed={dag_seed} include_leader_percentage={include_leader_percentage} \
             equivocations_per_authority={equivocations_per_authority}"
        );
    }
}

fn random_test_seed() -> u64 {
    telemetry_subscribers::init_for_testing();
    let mut seed: u64 = rand::thread_rng().r#gen();
    if let Ok(seed_str) = env::var("DAG_TEST_SEED")
        && let Ok(s) = seed_str.parse::<u64>()
    {
        seed = s;
        tracing::info!("Using DAG_TEST_SEED to override random seed: {seed}");
    }
    seed
}

fn random_test_rounds(default: u32) -> u32 {
    match env::var("DAG_TEST_ROUNDS").map(|rounds| rounds.parse::<u32>()) {
        Ok(Ok(rounds)) if rounds >= MIN_SHRUNK_ROUNDS => {
            tracing::info!("Using DAG_TEST_ROUNDS to override the number of rounds: {rounds}");
            rounds
        }
        _ => default,
    }
}

#[cfg(test)]
mod shrinking_tests {
    use super::*;

    const PARAMS: DagTestParams = DagTestParams {
        seed: 1,
        include_leader_percentage: 100,
        num_rounds: 1000,
        equivocations_per_authority: 0,
    };

    /// Shrinks `params` with a check that fails when `fails` holds.
    async fn shrink_with(
        params: DagTestParams,
        fails: impl Fn(DagTestParams) -> bool,
    ) -> DagTestParams {
        let (smallest, _) = shrink(params, "initial".to_string(), |candidate| {
            std::future::ready(fails(candidate).then(|| "failed".to_string()))
        })
        .await;
        smallest
    }

    #[tokio::test]
    async fn shrinks_the_rounds_to_the_smallest_failing_number() {
        for threshold in [3, 37, 500, 999, 1000] {
            let smallest = shrink_with(PARAMS, |p| p.num_rounds >= threshold).await;
            assert_eq!(smallest.num_rounds, threshold);
        }
    }

    #[tokio::test]
    async fn keeps_the_original_when_no_smaller_case_fails() {
        let smallest = shrink_with(PARAMS, |p| p == PARAMS).await;
        assert_eq!(smallest, PARAMS);
    }

    #[tokio::test]
    async fn keeps_one_equivocation_when_the_test_starts_with_some() {
        let params = DagTestParams {
            equivocations_per_authority: 3,
            ..PARAMS
        };
        let smallest = shrink_with(params, |p| p.num_rounds >= 10).await;
        assert_eq!(
            (smallest.num_rounds, smallest.equivocations_per_authority),
            (10, 1)
        );

        // A test without equivocations never gains any.
        let smallest = shrink_with(PARAMS, |p| p.num_rounds >= 10).await;
        assert_eq!(smallest.equivocations_per_authority, 0);
    }

    #[tokio::test]
    async fn reports_the_failure_of_the_smallest_case() {
        let (smallest, failure) = shrink(PARAMS, "original".to_string(), |candidate| {
            std::future::ready(
                (candidate.num_rounds >= 20).then(|| format!("{} rounds", candidate.num_rounds)),
            )
        })
        .await;
        assert_eq!((smallest.num_rounds, failure.as_str()), (20, "20 rounds"));
    }

    #[tokio::test]
    async fn a_passing_check_is_not_shrunk() {
        check_with_shrinking(PARAMS, |_| async {}).await;
    }

    #[tokio::test]
    #[should_panic(expected = "num_rounds: 37")]
    async fn a_failing_check_reports_the_smallest_case() {
        check_with_shrinking(PARAMS, |params| async move {
            assert!(
                params.num_rounds < 37,
                "too many rounds: {}",
                params.num_rounds
            );
        })
        .await;
    }

    #[test]
    fn failures_are_told_apart_by_their_words_before_the_first_number() {
        assert_eq!(
            failure_kind("More than one certified leader at wave 2 in [1]8"),
            "More than one certified leader at wave "
        );
        assert_eq!(failure_kind("No commits"), "No commits");
        assert_eq!(failure_kind("7 commits"), "7 commits");
    }

    /// A shorter DAG can fail a different check; shrinking must not follow it there.
    #[tokio::test]
    #[should_panic(expected = "num_rounds: 45")]
    async fn shrinking_does_not_follow_a_different_failure() {
        check_with_shrinking(PARAMS, |params| async move {
            if params.num_rounds >= 45 {
                panic!("conflicting commits at round {}", params.num_rounds);
            }
            panic!("only {} rounds, too few commits", params.num_rounds);
        })
        .await;
    }
}
