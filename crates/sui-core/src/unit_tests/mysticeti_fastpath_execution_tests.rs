// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::random_object_ref;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TestEffectsBuilder;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::transaction::VerifiedTransaction;
use sui_types::utils::to_sender_signed_transaction;

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::ExecutionEnv;
use crate::execution_scheduler::{ExecutionSchedulerAPI, SchedulingSource};
use crate::transaction_outputs::TransactionOutputs;

#[tokio::test]
async fn test_notify_read_fastpath_transaction_outputs() {
    let state = TestAuthorityBuilder::new().build().await;

    // Create a test transaction and effects.
    let (sender, sender_key) = get_account_key_pair();
    let tx_data = TestTransactionBuilder::new(sender, random_object_ref(), 1)
        .transfer_sui(None, sender)
        .build();
    let tx = VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key));
    let tx_digest = *tx.digest();
    let effects = TestEffectsBuilder::new(tx.data()).build();
    let effects_digest = effects.digest();

    let tx_outputs = Arc::new(TransactionOutputs::new_for_testing(tx, effects));

    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));

    // Write fastpath transaction outputs
    state
        .get_cache_writer()
        .write_fastpath_transaction_outputs(tx_outputs);

    // Verify that the transaction is marked as fastpath executed
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));

    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));

    // Test notification and reading of fastpath transaction outputs
    let outputs = state
        .get_transaction_cache_reader()
        .notify_read_fastpath_transaction_outputs(&[tx_digest])
        .await;
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].transaction.digest(), &tx_digest);

    // Test that outputs are not visible in regular transaction outputs until flushed
    let epoch_id = 0;
    state
        .get_cache_writer()
        .flush_fastpath_transaction_outputs(tx_digest, epoch_id);

    // Verify that the outputs are now available through regular transaction outputs
    let effects_digests = state
        .get_transaction_cache_reader()
        .notify_read_executed_effects_digests(&[tx_digest])
        .await;
    assert_eq!(effects_digests.len(), 1);
    assert_eq!(effects_digests[0], effects_digest);
}

#[tokio::test]
async fn test_fast_path_execution() {
    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();

    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;

    let tx_data = TestTransactionBuilder::new(
        sender,
        gas_object_ref,
        state.reference_gas_price_for_testing().unwrap(),
    )
    .transfer_sui(None, sender)
    .build();
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key)),
        0,
    );

    let (effects, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    let tx_digest = *cert.digest();
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));

    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));

    let state_clone = state.clone();
    let notify_read_task = tokio::spawn(async move {
        state_clone
            .get_transaction_cache_reader()
            .notify_read_executed_effects_digests(&[tx_digest])
            .await
    });

    state
        .get_cache_writer()
        .flush_fastpath_transaction_outputs(tx_digest, 0);

    let effects_digests = notify_read_task.await.unwrap();
    assert_eq!(effects_digests.len(), 1);
    assert_eq!(effects_digests[0], effects.digest());
}

#[tokio::test]
async fn test_fast_path_then_consensus_execution() {
    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;

    let tx_data = TestTransactionBuilder::new(
        sender,
        gas_object_ref,
        state.reference_gas_price_for_testing().unwrap(),
    )
    .transfer_sui(None, sender)
    .build();
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key)),
        0,
    );

    let (effects1, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    let (effects2, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    assert_eq!(effects1.digest(), effects2.digest());
    let tx_digest = cert.digest();
    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(tx_digest));
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_already_executed(tx_digest));
}

#[tokio::test]
async fn test_consensus_then_fast_path_execution() {
    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;

    let tx_data = TestTransactionBuilder::new(
        sender,
        gas_object_ref,
        state.reference_gas_price_for_testing().unwrap(),
    )
    .transfer_sui(None, sender)
    .build();
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key)),
        0,
    );

    let (effects1, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    let (effects2, _) = state
        .try_execute_immediately(
            &cert,
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
            &state.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    assert_eq!(effects1.digest(), effects2.digest());
    let tx_digest = cert.digest();
    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(tx_digest));
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_already_executed(tx_digest));
}

#[tokio::test]
async fn test_fast_path_then_consensus_execution_e2e() {
    telemetry_subscribers::init_for_testing();

    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;

    let tx_data = TestTransactionBuilder::new(
        sender,
        gas_object_ref,
        state.reference_gas_price_for_testing().unwrap(),
    )
    .transfer_sui(None, sender)
    .build();
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key)),
        0,
    );

    let tx_digest = *cert.digest();
    state.execution_scheduler().enqueue_transactions(
        vec![(
            cert.clone(),
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
        )],
        &state.epoch_store_for_testing(),
    );

    let outputs = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state
            .get_transaction_cache_reader()
            .notify_read_fastpath_transaction_outputs(&[*cert.digest()]),
    )
    .await
    .unwrap()
    .pop()
    .unwrap();
    assert_eq!(outputs.transaction.digest(), &tx_digest);

    assert!(state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));
    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));

    state.execution_scheduler().enqueue_transactions(
        vec![(
            cert.clone(),
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
        )],
        &state.epoch_store_for_testing(),
    );

    let effects_digest = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state
            .get_transaction_cache_reader()
            .notify_read_executed_effects_digests(&[tx_digest]),
    )
    .await
    .unwrap()
    .pop()
    .unwrap();

    assert_eq!(effects_digest, outputs.effects.digest());
    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));
}

#[tokio::test]
async fn test_consensus_then_fast_path_execution_e2e() {
    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;

    let tx_data = TestTransactionBuilder::new(
        sender,
        gas_object_ref,
        state.reference_gas_price_for_testing().unwrap(),
    )
    .transfer_sui(None, sender)
    .build();
    let cert = VerifiedExecutableTransaction::new_from_quorum_execution(
        VerifiedTransaction::new_unchecked(to_sender_signed_transaction(tx_data, &sender_key)),
        0,
    );

    let tx_digest = *cert.digest();
    state.execution_scheduler().enqueue_transactions(
        vec![(
            cert.clone(),
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::NonFastPath),
        )],
        &state.epoch_store_for_testing(),
    );

    state
        .get_transaction_cache_reader()
        .notify_read_executed_effects_digests(&[tx_digest])
        .await;

    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));

    state.execution_scheduler().enqueue_transactions(
        vec![(
            cert.clone(),
            ExecutionEnv::new().with_scheduling_source(SchedulingSource::MysticetiFastPath),
        )],
        &state.epoch_store_for_testing(),
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert!(!state
        .get_transaction_cache_reader()
        .is_tx_fastpath_executed(&tx_digest));
    assert!(state
        .get_transaction_cache_reader()
        .is_tx_already_executed(&tx_digest));
}
