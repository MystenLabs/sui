// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
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

async fn setup(
    gas_price: u64,
    proposer_epoch: Option<u64>,
    defer_unpaid_amplification: bool,
) -> (
    Arc<AuthorityState>,
    TestConsensusHandlerSetup<CheckpointServiceNoop>,
    ConsensusTransaction,
) {
    let (sender, keypair) = deterministic_random_account_key();
    let gas = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
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

async fn commit_copies(
    setup: &mut TestConsensusHandlerSetup<CheckpointServiceNoop>,
    transaction: &ConsensusTransaction,
    round: u64,
    copies: usize,
) {
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![transaction.clone(); copies],
            round,
            round * 1_000,
            round,
        ))
        .await;
}

#[tokio::test]
async fn cross_commit_copies_share_one_free_or_paid_allowance() {
    for (gas_price, copies, excess) in [
        (1, [1, 2, 1, 2], [0, 0, 1, 2]),
        // At 5x RGP, the signal allows six copies, including the SIP-45 race margin.
        (5, [1, 4, 1, 2], [0, 0, 0, 2]),
    ] {
        let (_state, mut setup, transaction) = setup(gas_price, None, false).await;
        let mut total_excess = 0;
        for (index, (copies, excess)) in copies.into_iter().zip(excess).enumerate() {
            commit_copies(&mut setup, &transaction, index as u64 + 1, copies).await;
            total_excess += excess;
            assert_eq!(
                setup
                    .metrics
                    .staggered_submission_excess_copies
                    .get_sample_sum(),
                total_excess as f64,
            );
            // Only the first occurrence enters the denominator, even when a later
            // commit contains several copies of this already-processed transaction.
            assert_eq!(
                setup.metrics.staggered_submission_duplication_ratio.get(),
                total_excess as f64,
            );
        }
    }
}

#[tokio::test]
async fn late_copies_activate_band_two() {
    let (state, mut setup, transaction) = setup(1, None, false).await;
    commit_copies(&mut setup, &transaction, 1, 1).await;
    assert_eq!(setup.metrics.staggered_submission_active.get(), 0);

    // The first commit finalized the transaction; only two free copies remain.
    commit_copies(&mut setup, &transaction, 2, 602).await;
    assert_eq!(
        setup
            .metrics
            .staggered_submission_excess_copies
            .get_sample_sum(),
        600.0,
    );
    assert_eq!(setup.metrics.staggered_submission_signal_band.get(), 2);
    assert_eq!(setup.metrics.staggered_submission_active.get(), 1);
    assert!(
        state
            .epoch_store_for_testing()
            .staggered_submission()
            .is_active()
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

    // Reloading the deferred transaction alongside two fresh copies must neither
    // add another unique transaction nor defer it based on the lifetime count (7).
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

    // Evict the user's entry. The store still knows it was processed, but cannot
    // recover how much of its paid/free allowance was spent before eviction.
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
    assert_eq!(commit_state.staggering_unique_user_txns, 1);
    assert_eq!(commit_state.staggering_excess_copies, 1);
    assert_eq!(
        commit_state
            .occurrence_counts
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![4]
    );
}
