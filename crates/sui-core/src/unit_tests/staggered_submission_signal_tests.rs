// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools as _;
use nonempty::NonEmpty;
use sui_protocol_config::ProtocolVersion;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::ObjectID,
    crypto::deterministic_random_account_key,
    object::Object,
    transaction::{AllowedProposers, Transaction, TransactionExpiration},
};

use super::*;
use crate::{
    authority::test_authority_builder::TestAuthorityBuilder,
    checkpoints::CheckpointServiceNoop,
    consensus_test_utils::{
        TestConsensusCommit, TestConsensusHandlerSetup, setup_consensus_handler_for_testing,
    },
};

/// Sets up a handler without the volume floor, so that a few copies can move the signal.
async fn setup(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    setup_with_config(
        gas_price,
        proposer_epoch,
        defer_unpaid_amplification,
        |_| {},
    )
    .await
}

async fn setup_with_min_consensus_user_transactions_per_second(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
    min_consensus_user_transactions_per_second: u64,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    setup_with_config(
        gas_price,
        proposer_epoch,
        defer_unpaid_amplification,
        |config| {
            config.set_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing(
                min_consensus_user_transactions_per_second,
            )
        },
    )
    .await
}

/// Sets up a handler without the volume floor. `configure` then adjusts the protocol
/// config.
async fn setup_with_config(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
    configure: impl FnOnce(&mut ProtocolConfig),
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config
        .set_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing(0);
    configure(&mut config);
    setup_with_protocol_config(
        gas_price,
        proposer_epoch,
        defer_unpaid_amplification,
        config,
    )
    .await
}

/// Sets up a handler at protocol version 144, whose signal counts commits, without the
/// volume floor.
async fn setup_with_commit_clock(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::new(144), Chain::Unknown);
    config
        .set_staggered_submission_signal_min_consensus_user_transactions_per_commit_for_testing(0);
    setup_with_protocol_config(
        gas_price,
        proposer_epoch,
        defer_unpaid_amplification,
        config,
    )
    .await
}

async fn setup_with_protocol_config(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
    mut config: ProtocolConfig,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    let (sender, keypair) = deterministic_random_account_key();
    let gas = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    config.set_allowed_proposers_for_testing(true);
    config.set_staggered_submission_signal_for_testing(true);
    config.set_defer_unpaid_amplification_for_testing(defer_unpaid_amplification);
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(std::slice::from_ref(&gas))
        .with_reference_gas_price(1)
        .with_protocol_config(config)
        .build()
        .await;

    let mut data = TestTransactionBuilder::new(sender, gas.compute_object_reference(), gas_price)
        .transfer_sui(None, sender)
        .build();
    if let Some(epoch) = proposer_epoch {
        *data.expiration_mut_for_testing() = TransactionExpiration::Validity {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: state.get_chain_identifier(),
            nonce: 0,
            allowed_proposers: Some(AllowedProposers {
                epoch,
                proposers: NonEmpty::new(0),
            }),
        };
    }
    let transaction = WithAliases::<VerifiedTransaction>::no_aliases(
        VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(data, vec![&keypair])),
    );
    let transaction =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());
    let handler = setup_consensus_handler_for_testing(&state).await;
    (state, handler, transaction)
}

fn epoch_start_ms(setup: &TestConsensusHandlerSetup<CheckpointServiceNoop>) -> u64 {
    setup
        .consensus_handler
        .epoch_store
        .epoch_start_config()
        .epoch_start_timestamp_ms()
}

/// Commits `copies` copies in round `round`, `round` seconds after the epoch start.
async fn commit_copies(
    setup: &mut TestConsensusHandlerSetup<CheckpointServiceNoop>,
    transaction: &ConsensusTransaction,
    round: u64,
    copies: usize,
) {
    // The handler raises commit timestamps below the epoch start to the epoch start.
    let timestamp_ms = epoch_start_ms(setup) + round * 1_000;
    commit_copies_at(setup, transaction, round, timestamp_ms, copies).await;
}

async fn commit_copies_at(
    setup: &mut TestConsensusHandlerSetup<CheckpointServiceNoop>,
    transaction: &ConsensusTransaction,
    round: u64,
    timestamp_ms: u64,
    copies: usize,
) {
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![transaction.clone(); copies],
            round,
            timestamp_ms,
            round,
        ))
        .await;
}

/// The signal clock of a handler test: the time clock of the latest version, or the
/// commit clock of version 144.
#[derive(Debug, Clone, Copy)]
enum HandlerClock {
    Time,
    Commits,
}

async fn setup_for_clock(
    clock: HandlerClock,
    gas_price: u64,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    match clock {
        HandlerClock::Time => setup(gas_price, None, false).await,
        HandlerClock::Commits => setup_with_commit_clock(gas_price, None, false).await,
    }
}

#[rstest::rstest]
#[case::time_clock(HandlerClock::Time, 1, [1, 2, 1, 2])]
#[case::commit_clock(HandlerClock::Commits, 1, [1, 2, 1, 2])]
// At 5x the reference gas price, five copies are paid for. The sixth copy is the first
// excess one.
#[case::paid_copies(HandlerClock::Time, 5, [1, 4, 1, 2])]
#[tokio::test]
async fn cross_commit_copies_share_one_free_or_paid_allowance(
    #[case] clock: HandlerClock,
    #[case] gas_price: u64,
    #[case] copies: [usize; 4],
) {
    let excess = [0, 0, 1, 2];
    let (_state, mut setup, transaction) = setup_for_clock(clock, gas_price).await;
    let mut total_excess = 0;
    for (index, (copies, excess)) in copies.into_iter().zip_eq(excess).enumerate() {
        commit_copies(&mut setup, &transaction, index as u64 + 1, copies).await;
        total_excess += excess;
        assert_eq!(
            setup
                .metrics
                .staggered_submission_excess_copies
                .get_sample_sum(),
            total_excess as f64,
        );
        // The transaction counts once as a unique transaction, so the ratio equals
        // the excess count.
        assert_eq!(
            setup.metrics.staggered_submission_duplication_ratio.get(),
            total_excess as f64,
        );
    }
}

#[rstest::rstest]
#[tokio::test]
async fn late_copies_activate_band_two(
    #[values(HandlerClock::Time, HandlerClock::Commits)] clock: HandlerClock,
) {
    let (state, mut setup, transaction) = setup_for_clock(clock, 1).await;
    let epoch_store = state.epoch_store_for_testing();
    let min_excess_copies = epoch_store
        .staggered_submission()
        .config()
        .min_excess_copies;
    commit_copies(&mut setup, &transaction, 1, 1).await;
    assert_eq!(setup.metrics.staggered_submission_active.get(), 0);

    // The first commit used one of the three free copies. Two remain, so all but two
    // of these copies are excess.
    commit_copies(&mut setup, &transaction, 2, min_excess_copies as usize + 2).await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_excess_copies
            .get_sample_sum(),
        min_excess_copies as f64,
    );
    assert_eq!(setup.metrics.staggered_submission_signal_band.get(), 2);
    assert_eq!(setup.metrics.staggered_submission_active.get(), 1);
    assert!(epoch_store.staggered_submission().is_active());
    // The signal left off on the second commit, one second after the first.
    let off_duration = |histogram: &prometheus::HistogramVec| {
        let histogram = histogram.with_label_values(&["off"]);
        (histogram.get_sample_count(), histogram.get_sample_sum())
    };
    assert_eq!(
        off_duration(&setup.metrics.staggered_submission_band_duration_commits),
        (1, 2.0)
    );
    assert_eq!(
        off_duration(&setup.metrics.staggered_submission_band_duration_ms),
        (1, 1_000.0)
    );
}

#[tokio::test]
async fn volume_floor_delays_activation_until_the_window_has_enough_transactions() {
    let min_consensus_user_transactions_per_second = 50;
    let (state, mut setup, transaction) = setup_with_min_consensus_user_transactions_per_second(
        1,
        None,
        false,
        min_consensus_user_transactions_per_second,
    )
    .await;
    let epoch_store = state.epoch_store_for_testing();
    let config = epoch_store.staggered_submission().config().clone();
    assert_eq!(
        config.min_consensus_user_txns,
        min_consensus_user_transactions_per_second
            * epoch_store
                .protocol_config()
                .staggered_submission_signal_window_ms()
            / 1_000
    );

    // The excess copies pass the excess floor and band 2's ratio. The window holds
    // three more user transactions than that, which is below the volume floor.
    commit_copies(&mut setup, &transaction, 1, 1).await;
    commit_copies(
        &mut setup,
        &transaction,
        2,
        config.min_excess_copies as usize + 2,
    )
    .await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_excess_copies
            .get_sample_sum(),
        config.min_excess_copies as f64,
    );
    assert_eq!(setup.metrics.staggered_submission_signal_band.get(), 0);
    assert_eq!(setup.metrics.staggered_submission_active.get(), 0);

    // The first copy and the two free duplicates count towards the volume floor.
    let window_consensus_user_txns = config.min_excess_copies + 3;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_window_consensus_user_transactions
            .get(),
        window_consensus_user_txns as i64
    );
    assert!(window_consensus_user_txns < config.min_consensus_user_txns);
    let missing = (config.min_consensus_user_txns - window_consensus_user_txns) as usize;
    commit_copies(&mut setup, &transaction, 3, missing - 1).await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_window_consensus_user_transactions
            .get(),
        config.min_consensus_user_txns as i64 - 1
    );
    assert_eq!(setup.metrics.staggered_submission_signal_band.get(), 0);
    commit_copies(&mut setup, &transaction, 4, 1).await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_window_consensus_user_transactions
            .get(),
        config.min_consensus_user_txns as i64
    );
    assert_eq!(setup.metrics.staggered_submission_signal_band.get(), 2);
    assert_eq!(setup.metrics.staggered_submission_active.get(), 1);
}

#[tokio::test]
async fn restricted_duplicates_count_towards_the_volume_floor() {
    let (_state, mut setup, transaction) =
        setup_with_min_consensus_user_transactions_per_second(1, Some(0), false, 1).await;
    commit_copies(&mut setup, &transaction, 1, 4).await;
    commit_copies(&mut setup, &transaction, 2, 2).await;
    // The ratio ignores restricted duplicates, but the window total counts all six
    // user transactions.
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        0.0
    );
    assert_eq!(
        setup
            .metrics
            .staggered_submission_window_consensus_user_transactions
            .get(),
        6
    );
}

#[tokio::test]
async fn restricted_copies_are_exempt_but_stale_proposer_lists_are_not() {
    for (proposer_epoch, expected_excess) in [(0, 0.0), (1, 3.0)] {
        let (_state, mut setup, transaction) = setup(1, Some(proposer_epoch), false).await;
        commit_copies(&mut setup, &transaction, 1, 4).await;
        commit_copies(&mut setup, &transaction, 2, 2).await;
        assert_eq!(
            setup
                .metrics
                .staggered_submission_excess_copies
                .get_sample_sum(),
            expected_excess,
        );
        assert_eq!(
            setup.metrics.staggered_submission_duplication_ratio.get(),
            expected_excess,
        );
    }
}

#[tokio::test]
async fn deferred_reload_does_not_recount_unique_or_change_deferral() {
    let (_state, mut setup, transaction) = setup(1, None, true).await;
    commit_copies(&mut setup, &transaction, 1, 5).await;
    assert_eq!(
        setup
            .metrics
            .consensus_handler_unpaid_amplification_deferrals
            .get(),
        1,
    );
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        2.0
    );

    // The deferred transaction is reloaded with two new copies. It must not count
    // as a new unique transaction. It must not be deferred again based on its
    // total count of seven copies.
    commit_copies(&mut setup, &transaction, 2, 2).await;
    assert_eq!(
        setup
            .metrics
            .consensus_handler_unpaid_amplification_deferrals
            .get(),
        1,
    );
    assert_eq!(
        setup
            .metrics
            .staggered_submission_excess_copies
            .get_sample_sum(),
        4.0,
    );
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        4.0
    );
    commit_copies(&mut setup, &transaction, 3, 0).await;
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        4.0
    );
}

#[tokio::test]
async fn lost_cache_history_regrants_allowance_without_recounting_unique() {
    let (state, mut setup, transaction) = setup(1, None, false).await;
    commit_copies(&mut setup, &transaction, 1, 2).await;
    let key = SequencedConsensusTransactionKey::External(transaction.key());
    assert!(
        state
            .epoch_store_for_testing()
            .is_consensus_message_processed(&key)
            .unwrap()
    );

    // Evict the transaction from the cache. The store still knows it was processed,
    // but the count of copies already seen is lost.
    setup
        .consensus_handler
        .processed_cache
        .resize(NonZeroUsize::new(1).unwrap());
    setup.consensus_handler.processed_cache.put(
        SequencedConsensusTransactionKey::System(TransactionDigest::random()),
        1,
    );
    commit_copies(&mut setup, &transaction, 2, 3).await;
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        0.0
    );
    commit_copies(&mut setup, &transaction, 3, 1).await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_excess_copies
            .get_sample_sum(),
        1.0,
    );
    assert_eq!(
        setup.metrics.staggered_submission_duplication_ratio.get(),
        1.0
    );
}

#[tokio::test]
async fn within_commit_eviction_preserves_known_counts_and_ignores_system_messages() {
    let (state, mut setup, transaction) = setup(1, None, false).await;
    let epoch_store = state.epoch_store_for_testing();
    let authority = *epoch_store.committee().authority_by_index(0).unwrap();
    let system = ConsensusTransaction::new_end_of_publish(authority);
    setup
        .consensus_handler
        .processed_cache
        .resize(NonZeroUsize::new(1).unwrap());
    let mut commit_state = CommitHandlerState::new(&epoch_store, 1);
    let commit_info = ConsensusCommitInfo::new_for_test(1, 1_000, None, true);
    let transactions = (0..4)
        .flat_map(|_| [transaction.clone(), system.clone()])
        .map(|tx| (SequencedConsensusTransactionKind::External(tx), 0))
        .collect();
    let unique = setup.consensus_handler.deduplicate_consensus_txns(
        &mut commit_state,
        &commit_info,
        transactions,
    );
    assert_eq!(unique.len(), 2);
    assert_eq!(commit_state.staggering_counts.unique_user_txns, 1);
    assert_eq!(commit_state.staggering_counts.excess_copies, 1);
    assert_eq!(
        commit_state
            .occurrence_counts
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![4]
    );
}

#[tokio::test]
async fn window_follows_commit_timestamps() {
    // The window lasts 2 s, and the gap cap is below it. `commit_copies` stamps round
    // `r` at `r` seconds after the epoch start.
    let (_state, mut setup, transaction) = setup_with_config(1, None, false, |config| {
        config.set_staggered_submission_signal_window_ms_for_testing(2_000);
        config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(1_500);
    })
    .await;
    let window_user_txns = setup
        .metrics
        .staggered_submission_window_consensus_user_transactions
        .clone();
    commit_copies(&mut setup, &transaction, 1, 4).await;
    commit_copies(&mut setup, &transaction, 2, 2).await;
    assert_eq!(window_user_txns.get(), 6);
    // The commit at 3 s is 2 s after the first one, which leaves the window.
    commit_copies(&mut setup, &transaction, 3, 1).await;
    assert_eq!(window_user_txns.get(), 3);
}

#[tokio::test]
async fn window_reads_commit_timestamps_raised_to_the_epoch_start() {
    let (_state, mut setup, transaction) = setup_with_config(1, None, false, |config| {
        config.set_staggered_submission_signal_window_ms_for_testing(3_000);
        config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(2_000);
    })
    .await;
    let window_user_txns = setup
        .metrics
        .staggered_submission_window_consensus_user_transactions
        .clone();
    let epoch_start_ms = epoch_start_ms(&setup);
    // Both timestamps are raised to the epoch start, so the second commit does not
    // advance the clock.
    commit_copies_at(&mut setup, &transaction, 1, epoch_start_ms - 10_000, 4).await;
    commit_copies_at(&mut setup, &transaction, 2, epoch_start_ms - 5_000, 2).await;
    assert_eq!(window_user_txns.get(), 6);
    // The clock is at 2 s, so both commits stay. With the raw timestamps the clock
    // would be at 4 s, and the first commit would have left.
    commit_copies_at(&mut setup, &transaction, 3, epoch_start_ms + 2_000, 1).await;
    assert_eq!(window_user_txns.get(), 7);
    commit_copies_at(&mut setup, &transaction, 4, epoch_start_ms + 3_000, 1).await;
    assert_eq!(window_user_txns.get(), 2);
}

#[tokio::test]
async fn dwell_follows_commit_time_and_caps_a_stall() {
    // The window lasts 3 s, the dwell 5 s, and one commit adds at most 1 s.
    let (state, mut setup, transaction) = setup_with_config(1, None, false, |config| {
        config.set_staggered_submission_signal_window_ms_for_testing(3_000);
        config.set_staggered_submission_signal_exit_dwell_ms_for_testing(5_000);
        config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(1_000);
    })
    .await;
    let min_excess_copies = state
        .epoch_store_for_testing()
        .staggered_submission()
        .config()
        .min_excess_copies;
    let band = setup.metrics.staggered_submission_signal_band.clone();
    let epoch_start_ms = epoch_start_ms(&setup);
    let at = |seconds: u64| epoch_start_ms + seconds * 1_000;
    commit_copies_at(&mut setup, &transaction, 1, at(1), 1).await;
    commit_copies_at(
        &mut setup,
        &transaction,
        2,
        at(2),
        min_excess_copies as usize + 2,
    )
    .await;
    assert_eq!(band.get(), 2);
    // The burst leaves the window at 5 s, which starts the dwell with 1 s.
    for second in 3..=5 {
        commit_copies_at(&mut setup, &transaction, second, at(second), 0).await;
        assert_eq!(band.get(), 2, "at {second} s");
    }
    // A 10 s stall adds only the 1 s cap.
    commit_copies_at(&mut setup, &transaction, 6, at(15), 0).await;
    assert_eq!(band.get(), 2);
    for (round, second) in [(7, 16), (8, 17)] {
        commit_copies_at(&mut setup, &transaction, round, at(second), 0).await;
        assert_eq!(band.get(), 2, "at {second} s");
    }
    // The fifth second of the dwell ends it.
    commit_copies_at(&mut setup, &transaction, 9, at(18), 0).await;
    assert_eq!(band.get(), 0);
}
