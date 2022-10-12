// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair, AuthoritySignature, SuiAuthoritySignature},
    error::SuiError,
    messages::{SignatureAggregator, TransactionData},
    object::Object,
    SUI_SYSTEM_STATE_OBJECT_ID,
};

use crate::authority::AuthorityState;
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
    let mut sigs = SignatureAggregator::try_new(transaction.clone(), &net.committee).unwrap();
    let mut cert = None;
    for state in &states {
        cert = sigs
            .append(
                state.name,
                AuthoritySignature::new(&transaction.signed_data, &*state.secret),
            )
            .unwrap();
    }
    let certificate = cert.unwrap();
    assert_eq!(
        state
            .handle_certificate(certificate.clone())
            .await
            .unwrap_err(),
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

fn enable_reconfig(states: &[Arc<AuthorityState>]) {
    for state in states {
        state.checkpoints.lock().enable_reconfig = true;
    }
}
