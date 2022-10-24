// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use narwhal_executor::ExecutionIndices;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use sui_types::crypto::AuthoritySignInfo;
use sui_types::messages::CertifiedTransaction;
use sui_types::messages_checkpoint::{
    AuthenticatedCheckpoint, CertifiedCheckpointSummary, CheckpointContents,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    error::SuiError,
    messages::TransactionData,
    object::Object,
    SUI_SYSTEM_STATE_OBJECT_ID,
};

use crate::authority::AuthorityState;
use crate::authority_active::execution_driver::PendCertificateForExecutionNoop;
use crate::checkpoints::causal_order_effects::TestEffectsStore;
use crate::checkpoints::reconstruction::SpanGraph;
use crate::{
    authority_active::ActiveAuthority,
    authority_aggregator::authority_aggregator_tests::init_local_authorities,
    checkpoints::{CheckpointLocals, CHECKPOINT_COUNT_PER_EPOCH},
    test_utils::to_sender_signed_transaction,
};

#[tokio::test]
async fn test_start_epoch_change() {
    // Create a sender, owning an object and a gas object.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let genesis_objects = vec![object.clone(), gas_object.clone()];
    // Create authority_aggregator and authority states.
    let (net, states, _) = init_local_authorities(4, genesis_objects.clone()).await;
    enable_reconfig(&states);
    let state = states[0].clone();

    // Check that we initialized the genesis epoch.
    let genesis_committee = state.committee_store().get_latest_committee();
    assert_eq!(genesis_committee.epoch, 0);

    // Set the checkpoint number to be near the end of epoch.

    let checkpoints = &state.checkpoints;
    checkpoints
        .lock()
        .set_locals_for_testing(CheckpointLocals {
            next_checkpoint: CHECKPOINT_COUNT_PER_EPOCH,
            proposal_next_transaction: None,
            next_transaction_sequence: 0,
            current_proposal: None,
            in_construction_checkpoint_seq: CHECKPOINT_COUNT_PER_EPOCH,
            in_construction_checkpoint: SpanGraph::mew(
                &genesis_committee,
                CHECKPOINT_COUNT_PER_EPOCH,
                &[],
            ),
        })
        .unwrap();
    // Create an active authority for the first authority state.
    let active =
        ActiveAuthority::new_with_ephemeral_storage_for_test(state.clone(), net.clone()).unwrap();
    // Make the high watermark differ from low watermark.
    let ticket = state.batch_notifier.ticket(false).unwrap();

    // Invoke start_epoch_change on the active authority.
    let epoch_change_started = Arc::new(AtomicBool::new(false));
    let epoch_change_started_copy = epoch_change_started.clone();
    let handle = tokio::spawn(async move {
        active.start_epoch_change().await.unwrap();
        epoch_change_started_copy.store(true, Ordering::SeqCst);
    });
    tokio::time::sleep(Duration::from_secs(3)).await;
    // Validator should now be halted, but epoch change hasn't finished because it's waiting for
    // tickets to be drained.
    assert!(state.is_halted());
    assert!(!epoch_change_started.load(Ordering::SeqCst));
    assert_eq!(checkpoints.lock().next_transaction_sequence_expected(), 0);

    // Drain ticket.
    ticket.notify();
    tokio::time::sleep(Duration::from_secs(3)).await;
    // After we drained ticket, epoch change should have started, as it will actively update
    // the newly processed transactions regardless whether batch service has picked up.
    assert!(epoch_change_started.load(Ordering::SeqCst));
    assert_eq!(checkpoints.lock().next_transaction_sequence_expected(), 1);

    handle.await.unwrap();

    // Test that when validator is halted, we cannot send any transaction.
    let tx_data = TransactionData::new_transfer(
        SuiAddress::default(),
        object.compute_object_reference(),
        sender,
        gas_object.compute_object_reference(),
        1000,
    );
    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    assert_eq!(
        state
            .handle_transaction(transaction.clone())
            .await
            .unwrap_err(),
        SuiError::ValidatorHaltedAtEpochEnd
    );

    // Test that when validator is halted, we cannot send any certificate.
    let sigs = states
        .iter()
        .map(|state| {
            AuthoritySignInfo::new(0, &transaction.signed_data, state.name, &*state.secret)
        })
        .collect();
    let certificate = CertifiedTransaction::new_with_auth_sign_infos(
        transaction.clone(),
        sigs,
        &genesis_committee,
    )
    .unwrap();
    assert_eq!(
        state.handle_certificate(&certificate).await.unwrap_err(),
        SuiError::ValidatorHaltedAtEpochEnd
    );

    // Test that for certificates that have finished execution and is about to write effects,
    // they will also fail to get a ticket for the commit.
    assert!(state.batch_notifier.ticket(false).is_err());
}

#[tokio::test]
async fn test_finish_epoch_change() {
    // Create authority_aggregator and authority states.
    let genesis_objects = vec![];
    let (net, states, _) = init_local_authorities(4, genesis_objects.clone()).await;
    enable_reconfig(&states);
    let actives: Vec<_> = states
        .iter()
        .map(|state| {
            ActiveAuthority::new_with_ephemeral_storage_for_test(state.clone(), net.clone())
                .unwrap()
        })
        .collect();

    let results: Vec<_> = states
        .iter()
        .zip(actives.iter())
        .map(|(state, active)| {
            async {
                let genesis_committee = state.committee_store().get_latest_committee();
                // Set the checkpoint number to be near the end of epoch.
                let mut locals = CheckpointLocals {
                    next_checkpoint: CHECKPOINT_COUNT_PER_EPOCH,
                    proposal_next_transaction: None,
                    next_transaction_sequence: 0,
                    current_proposal: None,
                    in_construction_checkpoint_seq: CHECKPOINT_COUNT_PER_EPOCH,
                    in_construction_checkpoint: SpanGraph::mew(
                        &genesis_committee,
                        CHECKPOINT_COUNT_PER_EPOCH,
                        &[],
                    ),
                };
                state
                    .checkpoints
                    .lock()
                    .set_locals_for_testing(locals.clone())
                    .unwrap();

                active.start_epoch_change().await.unwrap();

                locals.next_checkpoint += 1;
                state
                    .checkpoints
                    .lock()
                    .set_locals_for_testing(locals.clone())
                    .unwrap();

                active.finish_epoch_change().await.unwrap()
            }
        })
        .collect();
    futures::future::join_all(results).await;

    // Verify that epoch changed in every authority state.
    for active in actives {
        assert_eq!(active.state.epoch(), 1);
        assert_eq!(active.net.load().committee.epoch, 1);
        let latest_committee = active.state.committee_store().get_latest_committee();
        assert_eq!(latest_committee.epoch, 1);
        // Verify that validator is no longer halted.
        assert!(!active.state.is_halted());
        let system_state = active.state.get_sui_system_state_object().await.unwrap();
        assert_eq!(system_state.epoch, 1);
        let (_, tx_digest) = active
            .state
            .get_latest_parent_entry(SUI_SYSTEM_STATE_OBJECT_ID)
            .await
            .unwrap()
            .unwrap();
        let response = active
            .state
            .handle_transaction_info_request(tx_digest.into())
            .await
            .unwrap();
        assert!(response.signed_effects.is_some());
        assert!(response.certified_transaction.is_some());
        assert!(response.signed_effects.is_some());
    }
}

#[tokio::test]
async fn test_consensus_pause_after_last_fragment() {
    // Create authority_aggregator and authority states.
    let genesis_objects = vec![];
    let (net, states, _) = init_local_authorities(4, genesis_objects.clone()).await;
    enable_reconfig(&states);

    let proposals: Vec<_> = states
        .iter()
        .map(|state| {
            let genesis_committee = state.committee_store().get_latest_committee();
            // Set the checkpoint number to be about to construct the last second checkpoint.
            let locals = CheckpointLocals {
                next_checkpoint: CHECKPOINT_COUNT_PER_EPOCH - 1,
                proposal_next_transaction: None,
                next_transaction_sequence: 0,
                current_proposal: None,
                in_construction_checkpoint_seq: CHECKPOINT_COUNT_PER_EPOCH - 1,
                in_construction_checkpoint: SpanGraph::mew(
                    &genesis_committee,
                    CHECKPOINT_COUNT_PER_EPOCH - 1,
                    &[],
                ),
            };
            state
                .checkpoints
                .lock()
                .set_locals_for_testing(locals)
                .unwrap();
            assert!(!state
                .checkpoints
                .lock()
                .should_reject_consensus_transaction());
            state.checkpoints.lock().set_proposal(0).unwrap()
        })
        .collect();
    let fragment01 = proposals[0].fragment_with(&proposals[1]);
    let fragment12 = proposals[1].fragment_with(&proposals[2]);
    let mut index = ExecutionIndices::default();
    states.iter().for_each(|state| {
        // Send the first fragment to every validator, and make sure ater this, none of them
        // have a complete span graph, nor should they start rejecting consensus transactions.
        let mut cp = state.checkpoints.lock();
        cp.handle_internal_fragment(
            index.clone(),
            fragment01.clone(),
            PendCertificateForExecutionNoop,
            &net.committee,
        )
        .unwrap();
        index.next_transaction_index += 1;
        assert!(!cp.get_locals().in_construction_checkpoint.is_completed());
        assert!(!cp.should_reject_consensus_transaction());
    });
    // Send the second fragment only to validator 1-3, leaving validator 0 behind.
    // Validator 1-3 will complete the span graph. At this point they should all start rejecting
    // consensus transactions. They are each able to sign the checkpoint.
    let signed: Vec<_> = states
        .iter()
        .skip(1)
        .map(|state| {
            let mut cp = state.checkpoints.lock();
            cp.handle_internal_fragment(
                index.clone(),
                fragment12.clone(),
                PendCertificateForExecutionNoop,
                &net.committee,
            )
            .unwrap();
            index.next_transaction_index += 1;
            assert!(cp.get_locals().in_construction_checkpoint.is_completed());
            assert!(cp.should_reject_consensus_transaction());
            cp.sign_new_checkpoint(
                0,
                CHECKPOINT_COUNT_PER_EPOCH - 1,
                [].into_iter(),
                TestEffectsStore::default(),
                None,
            )
            .unwrap();
            if let AuthenticatedCheckpoint::Signed(s) = cp.latest_stored_checkpoint().unwrap() {
                s
            } else {
                unreachable!();
            }
        })
        .collect();
    let cert = CertifiedCheckpointSummary::aggregate(signed, &net.committee).unwrap();
    // Even after we processed the checkpoint cert on validator 0, it still does not have a
    // complete span graph, and hence not yet rejecting consensus transactions.
    let mut cp0 = states[0].checkpoints.lock();
    cp0.process_synced_checkpoint_certificate(
        &cert,
        &CheckpointContents::new_with_causally_ordered_transactions([].into_iter()),
        &net.committee,
    )
    .unwrap();
    assert!(!cp0.get_locals().in_construction_checkpoint.is_completed());
    assert!(!cp0.should_reject_consensus_transaction());
    states.iter().skip(1).for_each(|state| {
        let mut cp = state.checkpoints.lock();
        cp.promote_signed_checkpoint_to_cert(&cert, &net.committee)
            .unwrap();
        // Validator 1-3 will continue to reject consensus transactions after storing the cert.
        assert!(cp.should_reject_consensus_transaction());
        // The span graph is now empty, ready to construct the next checkpoint.
        assert!(!cp.get_locals().in_construction_checkpoint.is_completed());
    });
    // Now send the second fragment to validator 0. It will complete the span graph, and since it's
    // already at a new checkpoint, it will automatically clear the graph. It will also start
    // rejecting consensus transactions.
    cp0.handle_internal_fragment(
        index,
        fragment12,
        PendCertificateForExecutionNoop,
        &net.committee,
    )
    .unwrap();
    assert!(!cp0.get_locals().in_construction_checkpoint.is_completed());
    assert!(cp0.should_reject_consensus_transaction());
}

#[tokio::test]
async fn test_cross_epoch_effects_cert() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let genesis_objects = vec![Object::with_owner_for_testing(sender)];
    let (mut net, states, _) = init_local_authorities(4, genesis_objects.clone()).await;

    let object_ref = genesis_objects[0].compute_object_reference();
    let tx_data =
        TransactionData::new_transfer_sui(SuiAddress::default(), sender, None, object_ref, 1000);
    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    net.execute_transaction(&transaction).await.unwrap();
    for state in states {
        // Manually update each validator's epoch to the next one for testing purpose.
        let mut new_committee = (**state.committee.load()).clone();
        new_committee.epoch += 1;
        state.committee.store(Arc::new(new_committee));
    }
    // Also need to update the authority aggregator's committee.
    net.committee.epoch += 1;
    // Call to execute_transaction can still succeed.
    let (tx_cert, effects_cert) = net.execute_transaction(&transaction).await.unwrap();
    assert_eq!(tx_cert.auth_sign_info.epoch, 0);
    assert_eq!(effects_cert.auth_signature.epoch, 1);
}

fn enable_reconfig(states: &[Arc<AuthorityState>]) {
    for state in states {
        state.checkpoints.lock().enable_reconfig = true;
    }
}
