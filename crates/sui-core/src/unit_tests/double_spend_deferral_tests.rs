// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::consensus_adapter::consensus_tests::test_user_transaction;
use crate::consensus_test_utils::{self, TestConsensusCommit};
use std::time::Duration;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::ObjectID;
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::object::Object;

fn protocol_config_with_double_spend_deferral() -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    config.set_defer_owned_object_double_spend_for_testing(true);
    config
}

/// Two transactions in the same commit compete for the same owned object.
/// The first to acquire the lock wins but gets deferred (penalty for being contested).
/// The second is dropped (failed lock).
/// Verifying: only the prologue transaction is scheduled in the first commit;
/// the winner is deferred and scheduled in the second commit.
#[tokio::test]
async fn test_double_spend_winner_deferred() {
    telemetry_subscribers::init_for_testing();

    let protocol_config = protocol_config_with_double_spend_deferral();
    let (sender, keypair): (_, AccountKeyPair) = deterministic_random_account_key();

    // Two gas objects for two separate transactions.
    let gas_object_1 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let gas_object_2 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    // One contested owned object both transactions will try to use.
    let contested_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(1000)
        .with_protocol_config(protocol_config)
        .build()
        .await;
    authority_state
        .insert_genesis_objects(&[
            gas_object_1.clone(),
            gas_object_2.clone(),
            contested_object.clone(),
        ])
        .await;

    let consensus_setup =
        consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;
    let mut consensus_handler = consensus_setup.consensus_handler;
    let captured_transactions = consensus_setup.captured_transactions;

    // Build two transactions that use the same owned object.
    let tx1 = test_user_transaction(
        &authority_state,
        sender,
        &keypair,
        gas_object_1.clone(),
        vec![contested_object.clone()],
    )
    .await;
    let tx2 = test_user_transaction(
        &authority_state,
        sender,
        &keypair,
        gas_object_2.clone(),
        vec![contested_object.clone()],
    )
    .await;

    let consensus_transactions = vec![
        ConsensusTransaction::new_user_transaction_v2_message(
            &authority_state.name,
            tx1.clone().into(),
        ),
        ConsensusTransaction::new_user_transaction_v2_message(
            &authority_state.name,
            tx2.clone().into(),
        ),
    ];

    // Round 1: both transactions in the same commit.
    let commit = TestConsensusCommit::new(consensus_transactions, 1, 0, 0);
    consensus_handler.handle_consensus_commit(commit).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // The winner transaction should be deferred (not scheduled yet), and the loser should be
    // dropped. Only the consensus commit prologue should be scheduled in this round.
    let scheduled_round1 = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected at least one scheduler message"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns
    };

    // Only the prologue transaction should be scheduled (the winner got deferred,
    // the loser got dropped).
    assert_eq!(
        scheduled_round1.len(),
        1,
        "Expected only the prologue transaction in round 1, got {}",
        scheduled_round1.len()
    );

    // Round 2: empty commit - this should pick up the deferred transaction.
    let commit_round2 = TestConsensusCommit::empty(2, 100, 1);
    consensus_handler
        .handle_consensus_commit(commit_round2)
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // The deferred winner transaction should now be scheduled along with the round 2 prologue.
    let scheduled_round2 = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected scheduler message for round 2"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns
    };

    // Should have the prologue + the previously deferred winner transaction.
    assert_eq!(
        scheduled_round2.len(),
        2,
        "Expected prologue + deferred transaction in round 2, got {}",
        scheduled_round2.len()
    );
}

/// Same setup as above but with the feature flag disabled.
/// The winner should NOT be deferred - it should be scheduled immediately.
#[tokio::test]
async fn test_double_spend_no_deferral_when_disabled() {
    telemetry_subscribers::init_for_testing();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_defer_owned_object_double_spend_for_testing(false);

    let (sender, keypair): (_, AccountKeyPair) = deterministic_random_account_key();

    let gas_object_1 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let gas_object_2 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let contested_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(1000)
        .with_protocol_config(protocol_config)
        .build()
        .await;
    authority_state
        .insert_genesis_objects(&[
            gas_object_1.clone(),
            gas_object_2.clone(),
            contested_object.clone(),
        ])
        .await;

    let consensus_setup =
        consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;
    let mut consensus_handler = consensus_setup.consensus_handler;
    let captured_transactions = consensus_setup.captured_transactions;

    let tx1 = test_user_transaction(
        &authority_state,
        sender,
        &keypair,
        gas_object_1.clone(),
        vec![contested_object.clone()],
    )
    .await;
    let tx2 = test_user_transaction(
        &authority_state,
        sender,
        &keypair,
        gas_object_2.clone(),
        vec![contested_object.clone()],
    )
    .await;

    let consensus_transactions = vec![
        ConsensusTransaction::new_user_transaction_v2_message(
            &authority_state.name,
            tx1.clone().into(),
        ),
        ConsensusTransaction::new_user_transaction_v2_message(
            &authority_state.name,
            tx2.clone().into(),
        ),
    ];

    let commit = TestConsensusCommit::new(consensus_transactions, 1, 0, 0);
    consensus_handler.handle_consensus_commit(commit).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let scheduled = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected at least one scheduler message"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns
    };

    // Without the feature, the winner should be scheduled immediately (no deferral).
    // prologue + winner = 2 (loser is still dropped due to lock conflict).
    assert_eq!(
        scheduled.len(),
        2,
        "Expected prologue + winner transaction when deferral is disabled, got {}",
        scheduled.len()
    );
}

/// When only one transaction uses an owned object and there's no contention,
/// no deferral should happen even with the feature enabled.
#[tokio::test]
async fn test_no_deferral_without_contention() {
    telemetry_subscribers::init_for_testing();

    let protocol_config = protocol_config_with_double_spend_deferral();
    let (sender, keypair): (_, AccountKeyPair) = deterministic_random_account_key();

    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let owned_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(1000)
        .with_protocol_config(protocol_config)
        .build()
        .await;
    authority_state
        .insert_genesis_objects(&[gas_object.clone(), owned_object.clone()])
        .await;

    let consensus_setup =
        consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;
    let mut consensus_handler = consensus_setup.consensus_handler;
    let captured_transactions = consensus_setup.captured_transactions;

    let tx = test_user_transaction(
        &authority_state,
        sender,
        &keypair,
        gas_object.clone(),
        vec![owned_object.clone()],
    )
    .await;

    let consensus_transactions = vec![ConsensusTransaction::new_user_transaction_v2_message(
        &authority_state.name,
        tx.clone().into(),
    )];

    let commit = TestConsensusCommit::new(consensus_transactions, 1, 0, 0);
    consensus_handler.handle_consensus_commit(commit).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let scheduled = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected at least one scheduler message"
        );
        let (scheduled_txns, _) = captured.remove(0);
        scheduled_txns
    };

    // prologue + the single transaction (no contention, no deferral).
    assert_eq!(
        scheduled.len(),
        2,
        "Expected prologue + user transaction (no deferral), got {}",
        scheduled.len()
    );
}
