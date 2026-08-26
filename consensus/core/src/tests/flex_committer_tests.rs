// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use consensus_config::{AuthorityIndex, DIGEST_LENGTH};
use consensus_types::block::Round;
use parking_lot::RwLock;

use crate::{
    VerifiedBlock,
    block::{BlockAPI, Slot, TestBlock},
    commit::{CommitAPI, CommitIndex, Decision, LeaderStatus},
    context::Context,
    dag_state::DagState,
    flex_committer::{FlexCommitter, LeaderSlot, RoundState, sort_committed_blocks},
    leader_schedule_v3::NextCommitLeaderSchedule,
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
};

fn setup(num_authorities: usize) -> (Arc<Context>, Arc<RwLock<DagState>>, FlexCommitter) {
    let (mut context, _) = Context::new_for_test(num_authorities);
    context.protocol_config.set_enable_v3_for_testing(true);
    let context = Arc::new(context);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = FlexCommitter::new(context.clone(), dag_state.clone());
    (context, dag_state, committer)
}

fn next_commit_leader_schedule(allowed: Vec<AuthorityIndex>) -> NextCommitLeaderSchedule {
    NextCommitLeaderSchedule {
        // > 0 to force `maybe_refresh_pending_commit_state` to install our
        // allowed_leaders. The default index is 0.
        next_commit_index: 1,
        min_next_leader_round: 1,
        allowed_leaders: allowed,
    }
}

/// Schedule builder with explicit index / min round / allowed leaders.
fn schedule(
    next_commit_index: CommitIndex,
    min_next_leader_round: Round,
    allowed: &[u32],
) -> NextCommitLeaderSchedule {
    NextCommitLeaderSchedule {
        next_commit_index,
        min_next_leader_round,
        allowed_leaders: allowed
            .iter()
            .map(|a| AuthorityIndex::new_for_test(*a))
            .collect(),
    }
}

// ---- White-box helpers for unit tests on the internal commit state ----

fn commit_block(round: Round, author: u32) -> VerifiedBlock {
    VerifiedBlock::new_for_test(TestBlock::new(round, author).build())
}

fn skip(round: Round, author: u32) -> LeaderStatus {
    LeaderStatus::Skip(Slot::new(round, AuthorityIndex::new_for_test(author)))
}

fn undecided(round: Round, author: u32) -> LeaderStatus {
    LeaderStatus::Undecided(Slot::new(round, AuthorityIndex::new_for_test(author)))
}

/// Builds a `RoundState` directly from leader statuses, deriving `undecided_slots`
/// and `num_committed` the same way the production code maintains them.
fn round_state(round: Round, statuses: Vec<LeaderStatus>) -> RoundState {
    let leader_slots: Vec<LeaderSlot> = statuses
        .into_iter()
        .map(|leader_status| {
            let slot = match &leader_status {
                LeaderStatus::Commit(block) => Slot::new(block.round(), block.author()),
                LeaderStatus::Skip(slot) | LeaderStatus::Undecided(slot) => *slot,
            };
            // Decided slots in the test fixture are treated as direct decisions;
            // undecided slots carry no decision.
            let decision = match &leader_status {
                LeaderStatus::Commit(_) | LeaderStatus::Skip(_) => Some(Decision::Direct),
                LeaderStatus::Undecided(_) => None,
            };
            LeaderSlot {
                slot,
                leader_status,
                decision,
            }
        })
        .collect();
    let undecided_slots: BTreeSet<Slot> = leader_slots
        .iter()
        .filter(|s| matches!(s.leader_status, LeaderStatus::Undecided(_)))
        .map(|s| s.slot)
        .collect();
    let num_committed = leader_slots
        .iter()
        .filter(|s| matches!(s.leader_status, LeaderStatus::Commit(_)))
        .count();
    RoundState {
        round,
        leader_slots,
        undecided_slots,
        num_committed,
    }
}

/// Installs `rounds` directly into the committer's pending state, starting at
/// `min_next_leader_round`. Bypasses DAG-driven decision so that
/// `find_anchor_block` / `find_commit_leader_round` can be unit-tested against
/// arbitrary leader-status sequences.
fn install_rounds(
    committer: &mut FlexCommitter,
    min_next_leader_round: Round,
    rounds: Vec<RoundState>,
) {
    committer.maybe_refresh_pending_commit_state(schedule(1, min_next_leader_round, &[0]));
    committer.pending_commit_state.rounds = rounds.into();
}

fn build_blocks(rounds_x_authorities: &[(Round, u32)]) -> Vec<VerifiedBlock> {
    rounds_x_authorities
        .iter()
        .map(|(r, a)| VerifiedBlock::new_for_test(TestBlock::new(*r, *a).build()))
        .collect()
}

/// `committed_leaders_total` counts of one authority, per commit type.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LeaderCounts {
    direct_commit: u64,
    direct_skip: u64,
    indirect_skip: u64,
}

/// Reads the leader commit counters of `authority`. Each test gets its own metrics
/// registry, so the counts start at zero.
fn leader_counts(context: &Context, authority: u32) -> LeaderCounts {
    let leader_host = &context
        .committee
        .authority(AuthorityIndex::new_for_test(authority))
        .hostname;
    let count = |commit_type: &str| {
        context
            .metrics
            .node_metrics
            .committed_leaders_total
            .with_label_values(&[leader_host, commit_type])
            .get()
    };
    LeaderCounts {
        direct_commit: count("direct-commit"),
        direct_skip: count("direct-skip"),
        indirect_skip: count("indirect-skip"),
    }
}

// =================== Unit tests ===================

#[tokio::test]
async fn committed_blocks_are_total_ordered_and_round_major() {
    let inputs = build_blocks(&[
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 3),
        (2, 0),
        (2, 1),
        (2, 2),
        (2, 3),
    ]);
    let seed = [7u8; DIGEST_LENGTH];

    let mut a = inputs.clone();
    let mut b = inputs.clone();
    b.reverse();
    let mut c = inputs.clone();
    c.rotate_left(3);

    sort_committed_blocks(&mut a, &seed);
    sort_committed_blocks(&mut b, &seed);
    sort_committed_blocks(&mut c, &seed);

    let refs = |v: &[VerifiedBlock]| v.iter().map(|b| b.reference()).collect::<Vec<_>>();
    assert_eq!(
        refs(&a),
        refs(&b),
        "sort must be deterministic regardless of input order"
    );
    assert_eq!(refs(&a), refs(&c));

    // Round-major, ascending.
    let rounds: Vec<Round> = a.iter().map(|b| b.round()).collect();
    assert!(
        rounds.windows(2).all(|w| w[0] <= w[1]),
        "rounds must be non-decreasing"
    );
}

#[tokio::test]
async fn committed_blocks_within_round_are_not_author_ascending() {
    // With 4 distinct authorities in one round, a meaningful shuffle will
    // not leave them in author-ascending order for every seed.
    let inputs = build_blocks(&[(1, 0), (1, 1), (1, 2), (1, 3)]);

    // Try several seeds — at least one should scramble the author order.
    let mut any_scrambled = false;
    for s in 0u8..16 {
        let seed = [s; DIGEST_LENGTH];
        let mut v = inputs.clone();
        sort_committed_blocks(&mut v, &seed);
        let authors: Vec<u32> = v.iter().map(|b| b.author().value() as u32).collect();
        if authors != vec![0, 1, 2, 3] {
            any_scrambled = true;
            break;
        }
    }
    assert!(
        any_scrambled,
        "v3 sort should not always leave authors in ascending order"
    );
}

/// The within-round order must depend on the seed; a sort that ignored the seed
/// (e.g. ordering by block digest alone) would be deterministic but seed-blind.
#[tokio::test]
async fn committed_blocks_within_round_order_depends_on_seed() {
    let inputs = build_blocks(&[(1, 0), (1, 1), (1, 2), (1, 3)]);
    let order_for = |seed: [u8; DIGEST_LENGTH]| {
        let mut v = inputs.clone();
        sort_committed_blocks(&mut v, &seed);
        v.iter().map(|b| b.reference()).collect::<Vec<_>>()
    };

    let baseline = order_for([0u8; DIGEST_LENGTH]);
    let changed = (1u8..=64).any(|s| order_for([s; DIGEST_LENGTH]) != baseline);
    assert!(changed, "within-round order must depend on the seed");
}

/// Refreshing the same schedule state is a no-op.
#[tokio::test]
async fn maybe_refresh_is_noop_on_same_index() {
    let (_context, _dag_state, mut committer) = setup(4);
    let next_schedule = schedule(1, 1, &[0]);
    committer.maybe_refresh_pending_commit_state(next_schedule.clone());
    committer.pending_commit_state.get_or_create_round_state(3);
    let rounds_len = committer.pending_commit_state.rounds.len();
    assert!(rounds_len > 0);

    committer.maybe_refresh_pending_commit_state(next_schedule);
    assert_eq!(committer.pending_commit_state.rounds.len(), rounds_len);
}

/// A higher commit index keeps cached rounds when the ordered leader schedule is unchanged.
#[tokio::test]
async fn maybe_refresh_rebases_on_higher_index_with_same_schedule() {
    let (_context, _dag_state, mut committer) = setup(4);
    committer.maybe_refresh_pending_commit_state(schedule(1, 1, &[0, 1]));
    committer.pending_commit_state.get_or_create_round_state(5);
    let retained_slot = committer
        .pending_commit_state
        .get_round_state(4)
        .leader_slots[0]
        .slot;
    committer
        .pending_commit_state
        .get_round_state(4)
        .update_slot_decision(
            retained_slot,
            LeaderStatus::Skip(retained_slot),
            Decision::Direct,
        );

    committer.maybe_refresh_pending_commit_state(schedule(4, 4, &[0, 1]));

    let retained_rounds = committer
        .pending_commit_state
        .rounds
        .iter()
        .map(|state| state.round)
        .collect::<Vec<_>>();
    assert_eq!(retained_rounds, vec![4, 5]);
    let retained_state = committer.pending_commit_state.get_round_state(4);
    assert!(!retained_state.undecided_slots.contains(&retained_slot));
    assert_eq!(
        retained_state
            .leader_slots
            .iter()
            .find(|state| state.slot == retained_slot)
            .unwrap()
            .decision,
        Some(Decision::Direct),
    );
    let installed = &committer.pending_commit_state.next_commit_leaders;
    assert_eq!(installed.next_commit_index, 4);
    assert_eq!(installed.min_next_leader_round, 4);
    assert_eq!(installed.allowed_leaders.len(), 2);
}

/// A different ordered leader schedule clears all cached round decisions.
#[tokio::test]
async fn maybe_refresh_resets_on_schedule_change() {
    let (_context, _dag_state, mut committer) = setup(4);
    committer.maybe_refresh_pending_commit_state(schedule(1, 1, &[0, 1, 2]));
    committer.pending_commit_state.get_or_create_round_state(3);
    assert!(!committer.pending_commit_state.rounds.is_empty());

    committer.maybe_refresh_pending_commit_state(schedule(2, 2, &[1, 0, 2]));

    assert!(committer.pending_commit_state.rounds.is_empty());
    let installed = &committer.pending_commit_state.next_commit_leaders;
    assert_eq!(installed.next_commit_index, 2);
    assert_eq!(installed.min_next_leader_round, 2);
    assert_eq!(
        installed.allowed_leaders,
        vec![
            AuthorityIndex::new_for_test(1),
            AuthorityIndex::new_for_test(0),
            AuthorityIndex::new_for_test(2),
        ]
    );
}

/// `next_commit_index` must move forward; a lower index panics.
#[tokio::test]
#[should_panic(expected = "next_commit_index should only move forward")]
async fn maybe_refresh_panics_on_lower_index() {
    let (_context, _dag_state, mut committer) = setup(4);
    committer.maybe_refresh_pending_commit_state(schedule(3, 1, &[0]));
    committer.maybe_refresh_pending_commit_state(schedule(1, 1, &[0]));
}

/// The first committed leader, scanning from `start_round`, becomes the anchor.
#[tokio::test]
async fn find_anchor_block_returns_first_committed() {
    let (_context, _dag_state, mut committer) = setup(4);
    let anchor = commit_block(2, 1);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0)]),
            round_state(2, vec![LeaderStatus::Commit(anchor.clone())]),
            round_state(3, vec![LeaderStatus::Commit(commit_block(3, 2))]),
        ],
    );
    assert_eq!(
        committer.find_anchor_block(1).unwrap().reference(),
        anchor.reference(),
    );
}

/// An undecided slot before any commit means no anchor is available.
#[tokio::test]
async fn find_anchor_block_none_when_undecided_first() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![undecided(1, 0)]),
            round_state(2, vec![LeaderStatus::Commit(commit_block(2, 1))]),
        ],
    );
    assert!(committer.find_anchor_block(1).is_none());
}

/// Skipped slots — within and across rounds — are passed over until a commit.
#[tokio::test]
async fn find_anchor_block_skips_past_skipped_slots() {
    let (_context, _dag_state, mut committer) = setup(4);
    let anchor = commit_block(2, 3);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0), skip(1, 1)]),
            round_state(2, vec![skip(2, 2), LeaderStatus::Commit(anchor.clone())]),
        ],
    );
    assert_eq!(
        committer.find_anchor_block(1).unwrap().reference(),
        anchor.reference(),
    );
}

/// `start_round` skips earlier rounds entirely, even committed ones.
#[tokio::test]
async fn find_anchor_block_respects_start_round() {
    let (_context, _dag_state, mut committer) = setup(4);
    let later = commit_block(2, 0);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![LeaderStatus::Commit(commit_block(1, 0))]),
            round_state(2, vec![LeaderStatus::Commit(later.clone())]),
        ],
    );
    assert_eq!(
        committer.find_anchor_block(2).unwrap().reference(),
        later.reference(),
    );
}

/// All-skip rounds yield no anchor.
#[tokio::test]
async fn find_anchor_block_none_when_all_skipped() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0)]),
            round_state(2, vec![skip(2, 0)]),
        ],
    );
    assert!(committer.find_anchor_block(1).is_none());
}

/// An undecided slot blocks committing, even if a later round is committed.
#[tokio::test]
async fn find_commit_leader_round_none_when_undecided() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![undecided(1, 0)]),
            round_state(2, vec![LeaderStatus::Commit(commit_block(2, 0))]),
        ],
    );
    assert!(committer.find_commit_leader_round().is_none());
}

/// The earliest fully-decided round with a committed leader is the commit round.
#[tokio::test]
async fn find_commit_leader_round_returns_first_committed_round() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![LeaderStatus::Commit(commit_block(1, 0))]),
            round_state(2, vec![LeaderStatus::Commit(commit_block(2, 0))]),
        ],
    );
    assert_eq!(committer.find_commit_leader_round(), Some(1));
}

/// Fully-decided all-skip rounds are passed over to a later committed round.
#[tokio::test]
async fn find_commit_leader_round_skips_all_skip_rounds() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0)]),
            round_state(2, vec![skip(2, 0), skip(2, 1)]),
            round_state(3, vec![LeaderStatus::Commit(commit_block(3, 0))]),
        ],
    );
    assert_eq!(committer.find_commit_leader_round(), Some(3));
}

/// A later undecided round blocks committing, even past earlier all-skip rounds.
#[tokio::test]
async fn find_commit_leader_round_none_when_later_round_undecided() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0)]),
            round_state(2, vec![undecided(2, 0)]),
        ],
    );
    assert!(committer.find_commit_leader_round().is_none());
}

/// A round with a mix of committed and undecided slots is not yet committable.
#[tokio::test]
async fn find_commit_leader_round_none_when_round_partially_decided() {
    let (_context, _dag_state, mut committer) = setup(4);
    install_rounds(
        &mut committer,
        1,
        vec![round_state(
            1,
            vec![LeaderStatus::Commit(commit_block(1, 0)), undecided(1, 1)],
        )],
    );
    assert!(committer.find_commit_leader_round().is_none());
}

/// Every decided slot through the commit leader round is reported, and no later round.
#[tokio::test]
async fn report_decided_prefix_metrics_reports_through_commit_round() {
    let (context, _dag_state, mut committer) = setup(4);
    let mut indirect_skip = round_state(2, vec![skip(2, 0)]);
    indirect_skip.leader_slots[0].decision = Some(Decision::Indirect);
    install_rounds(
        &mut committer,
        1,
        vec![
            round_state(1, vec![skip(1, 0)]),
            indirect_skip,
            round_state(3, vec![LeaderStatus::Commit(commit_block(3, 0))]),
            round_state(4, vec![skip(4, 0)]),
        ],
    );

    committer.report_decided_prefix_metrics(3);

    // Round 4 is above the commit leader round, so it stays unreported. The next
    // commit reports it, after the refresh drops rounds 1 to 3.
    assert_eq!(
        leader_counts(&context, 0),
        LeaderCounts {
            direct_commit: 1,
            direct_skip: 1,
            indirect_skip: 1,
        },
    );
}

// =================== Functional `try_commit` tests ===================

/// One leader per round, fully connected DAG → committer emits a commit
/// rooted at that leader.
#[tokio::test]
async fn try_commit_single_leader_committed() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 2);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit when round 1 is fully voted");

    assert_eq!(commit.leader().round, 1);
    assert_eq!(commit.leader().author, AuthorityIndex::new_for_test(0));
    assert!(
        subdag
            .blocks
            .iter()
            .any(|b| b.round() == 1 && b.author() == AuthorityIndex::new_for_test(0)),
        "sub-dag must include the leader block",
    );
}

/// One leader per round, no round-2 block references it → all slots Skip,
/// no commit emitted.
#[tokio::test]
async fn try_commit_single_leader_all_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    build_dag(context.clone(), dag_state, Some(refs_without_leader), 2);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    assert!(committer.try_commit(next).is_none());

    // Leader metrics are reported with the commit that covers the round, so a
    // round that never reaches a commit reports nothing.
    assert_eq!(leader_counts(&context, 0), LeaderCounts::default());
}

/// One leader per round, no round-2 blocks at all → slot Undecided, no
/// commit emitted (and no anchor available for indirect commit).
#[tokio::test]
async fn try_commit_single_leader_undecided() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 1);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    assert!(committer.try_commit(next).is_none());
}

/// Commits must be emitted in round order: while an earlier round still has
/// Undecided slots, no later round may be committed — even if the later
/// round is fully decided and has a committed leader.
///
/// DAG shape (the leader at every round is authority 0):
///   - Round 1: full layer (4 blocks).
///   - Round 2: split layer — authorities 0, 1 reference all of round 1
///     (vote for the round-1 leader), authorities 2, 3 omit it (blame).
///     Stake for / against the round-1 leader is 2/2, neither side reaches
///     quorum — round 1 stays Undecided.
///   - Round 3: full layer on top of round 2, so the round-2 leader has 4
///     votes and would direct-commit on its own.
///
/// `try_commit` must therefore return `None`: the round-1 ambiguity blocks
/// emitting the round-2 commit until indirect-commit can resolve round 1.
#[tokio::test]
async fn try_commit_does_not_skip_undecided_prefix_round() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);
    let leader = AuthorityIndex::new_for_test(0);

    // Round 1: full layer.
    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_round_1_without_leader: Vec<_> = refs_round_1
        .iter()
        .copied()
        .filter(|r| r.author != leader)
        .collect();

    // Round 2: 2 voters for the round-1 leader, 2 blames against it.
    let mut authorities = context.committee.authorities();
    let mut connections = Vec::with_capacity(4);
    for _ in 0..2 {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    for _ in 0..2 {
        connections.push((
            authorities.next().unwrap().0,
            refs_round_1_without_leader.clone(),
        ));
    }
    let refs_round_2 = build_dag_layer(connections, dag_state.clone());

    // Round 3: full layer — the round-2 leader has a quorum of votes.
    build_dag(context.clone(), dag_state, Some(refs_round_2), 3);

    let next = next_commit_leader_schedule(vec![leader]);
    assert!(
        committer.try_commit(next).is_none(),
        "round 2 must not be committed while round 1 remains undecided",
    );
}

/// All four authorities are leaders at round 1, fully connected DAG → one
/// commit emitted whose sub-dag bundles every committed leader block.
#[tokio::test]
async fn try_commit_multi_leader_all_committed() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 2);

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit when all leaders fully voted");

    assert_eq!(commit.leader().round, 1);
    let round_1_authors: BTreeSet<AuthorityIndex> = subdag
        .blocks
        .iter()
        .filter(|b| b.round() == 1)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (0..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(round_1_authors, expected);
}

/// All four authorities are leaders at round 1; round 2 omits authority-0's
/// block. Three slots Commit + one slot Skip — the round is still
/// committable, and the sub-dag excludes the skipped leader.
#[tokio::test]
async fn try_commit_multi_leader_some_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader_0: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    build_dag(context, dag_state, Some(refs_without_leader_0), 2);

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit — three leaders committed, one skipped");

    assert_eq!(commit.leader().round, 1);
    assert_ne!(
        commit.leader().author,
        AuthorityIndex::new_for_test(0),
        "skipped leader must not be the named commit leader",
    );
    let round_1_authors: BTreeSet<AuthorityIndex> = subdag
        .blocks
        .iter()
        .filter(|b| b.round() == 1)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (1..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(
        round_1_authors, expected,
        "skipped leader's block is not in the sub-dag",
    );
}

/// All four authorities are leaders, but round 2 is a "diagonal" — each
/// authority's round-2 block references only its own round-1 block. Every
/// leader gets exactly one vote and three blames → all slots Skip → no
/// commit.
#[tokio::test]
async fn try_commit_multi_leader_all_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);

    let connections: Vec<_> = context
        .committee
        .authorities()
        .map(|(idx, _)| {
            let own = refs_round_1
                .iter()
                .find(|r| r.author == idx)
                .copied()
                .expect("each authority has a round-1 block");
            (idx, vec![own])
        })
        .collect();
    build_dag_layer(connections, dag_state.clone());

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    assert!(committer.try_commit(next).is_none());
}

/// All four authorities are leaders at round 2, fully connected DAG → every
/// leader's DFS walks into the same four round-1 blocks. Round 1 is never a
/// leader round itself (min_next_leader_round = 2), so those blocks are only
/// ever discovered as ancestors — this exercises the case where a later
/// leader's DFS re-encounters an ancestor a previous leader's DFS, within
/// the same `build_commit` call, already committed.
#[tokio::test]
async fn try_commit_multi_leader_shared_ancestor_visited_once() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 3);

    let next = schedule(1, 2, &[0, 1, 2, 3]);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit when round 2 is fully voted");

    assert_eq!(commit.leader().round, 2);

    let refs: Vec<_> = subdag.blocks.iter().map(|b| b.reference()).collect();
    let unique: BTreeSet<_> = refs.iter().copied().collect();
    assert_eq!(
        refs.len(),
        unique.len(),
        "a shared ancestor must not be committed twice"
    );

    let round_1_authors: BTreeSet<AuthorityIndex> = subdag
        .blocks
        .iter()
        .filter(|b| b.round() == 1)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (0..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(
        round_1_authors, expected,
        "each shared round-1 ancestor must still be included exactly once",
    );
}

/// Retained round decisions report one metric for each local commit.
#[tokio::test]
async fn try_commit_retained_rounds_report_metrics_once() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);
    build_dag(context.clone(), dag_state, None, 4);

    // The schedule keeps the same leader, so each refresh retains the round state
    // instead of resetting it.
    for commit_index in 1..=3 {
        let (commit, _) = committer
            .try_commit(schedule(commit_index, commit_index, &[0]))
            .expect("the fully connected DAG must produce a commit");
        assert_eq!(commit.leader().round, commit_index);
    }

    assert_eq!(
        leader_counts(&context, 0),
        LeaderCounts {
            direct_commit: 3,
            ..Default::default()
        },
    );
}

/// Drives the committer across three schedules — varying the allowed-leader
/// count each time — with a fully-skipped leader round in the middle.
///
/// DAG (4 authorities, quorum 3):
///   - Rounds 1, 2: full layers.
///   - Round 3: every block omits authority-0's round-2 block (blames it), so
///     the round-2 leader 0 collects a quorum of blames.
///   - Rounds 4, 5: full layers.
///
/// Schedule 1 (idx 1, min 1, leaders {0,1}): round 1 fully voted → commit round 1.
/// Schedule 2 (idx 2, min 2, leaders {0}):   round-2 leader 0 is blamed → round 2
///   is fully skipped; round-3 leader 0 is voted by round 4 → commit round 3.
/// Schedule 3 (idx 3, min 4, leaders {0,1,2,3}): round 4 fully voted → commit
///   round 4 with all four leaders.
#[tokio::test]
async fn try_commit_multiple_schedules_with_skipped_round() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    // Rounds 1 and 2: fully connected.
    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_round_2 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_1), 2);

    // Round 3: every authority omits authority-0's round-2 block, so round-2
    // leader 0 is blamed by a quorum and will be skipped.
    let refs_round_2_without_0: Vec<_> = refs_round_2
        .iter()
        .copied()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    let connections: Vec<_> = context
        .committee
        .authorities()
        .map(|(idx, _)| (idx, refs_round_2_without_0.clone()))
        .collect();
    let refs_round_3 = build_dag_layer(connections, dag_state.clone());

    // Rounds 4 and 5: fully connected on top of round 3.
    let refs_round_4 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_3), 4);
    build_dag(context.clone(), dag_state.clone(), Some(refs_round_4), 5);

    // Schedule 1: two leaders at round 1, both committed.
    let (commit1, _) = committer
        .try_commit(schedule(1, 1, &[0, 1]))
        .expect("round 1 fully voted → commit");
    assert_eq!(commit1.leader().round, 1);

    // Schedule 2: single leader. Round 2 is fully skipped (leader 0 blamed),
    // so the commit lands on round 3.
    let (commit2, _) = committer
        .try_commit(schedule(2, 2, &[0]))
        .expect("round 2 skipped, round 3 committed");
    assert_eq!(
        commit2.leader().round,
        3,
        "round 2 was fully skipped, so the commit is round 3",
    );

    // Schedule 3: four leaders at round 4, all committed.
    let (commit3, subdag3) = committer
        .try_commit(schedule(3, 4, &[0, 1, 2, 3]))
        .expect("round 4 fully voted → commit");
    assert_eq!(commit3.leader().round, 4);
    let round_4_leaders: BTreeSet<AuthorityIndex> = subdag3
        .blocks
        .iter()
        .filter(|b| b.round() == 4)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (0..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(
        round_4_leaders, expected,
        "all four round-4 leaders committed",
    );

    // Leader 0 commits at rounds 1, 3 and 4, and is skipped at round 2. The skip is
    // reported by the round-3 commit, which is the commit that covers round 2.
    assert_eq!(
        leader_counts(&context, 0),
        LeaderCounts {
            direct_commit: 3,
            direct_skip: 1,
            indirect_skip: 0,
        },
    );
}
