// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_active::ActiveAuthority, checkpoints::checkpoint_tests::TestSetup};

use crate::authority_active::checkpoint_driver::CheckpointMetrics;
use std::sync::Arc;
use std::time::Duration;

use sui_adapter::genesis;
use sui_types::crypto::AccountKeyPair;
use sui_types::{crypto::get_key_pair, messages::ExecutionStatus, object::Object};

//use super::super::AuthorityState;
use crate::authority_aggregator::authority_aggregator_tests::{
    crate_object_move_transaction, do_cert, do_transaction, extract_cert, get_latest_ref,
    init_local_authorities, transfer_object_move_transaction,
};
use crate::checkpoints::checkpoint_tests::checkpoint_tests_setup;
use crate::test_utils::wait_for_tx;

use tracing::info;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_storage_notify() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();

    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let active_state = Arc::new(
            ActiveAuthority::new_with_ephemeral_storage_for_test(
                inner_state.authority.clone(),
                inner_agg,
            )
            .unwrap(),
        );
        let _active_handle = active_state
            .spawn_checkpoint_process(CheckpointMetrics::new_for_tests(), false)
            .await;
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.effects().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Insert the certificates
    let num_certs = certs.len();
    authority_state
        .database
        .add_pending_certificates(
            certs
                .into_iter()
                .map(|cert| (*cert.digest(), Some(cert)))
                .collect(),
        )
        .expect("Storage is ok");

    tokio::task::yield_now().await;

    // Wait for a notification (must arrive)
    authority_state.database.wait_for_new_pending().await;
    // get back the certificates
    let certs_back = authority_state
        .database
        .get_pending_digests()
        .expect("DB should be there");
    assert_eq!(num_certs, certs_back.len());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_full() {
    // use telemetry_subscribers::init_for_testing;
    // init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();

    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let _active_handle = tokio::task::spawn(async move {
            let active_state = Arc::new(
                ActiveAuthority::new_with_ephemeral_storage_for_test(
                    inner_state.authority.clone(),
                    inner_agg,
                )
                .unwrap(),
            );

            active_state.clone().spawn_execute_process().await;
            active_state
                .spawn_checkpoint_process(CheckpointMetrics::new_for_tests(), false)
                .await;
        });
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.effects().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Insert the certificates
    let num_certs = certs.len();
    authority_state
        .database
        .add_pending_certificates(
            certs
                .into_iter()
                .map(|cert| (*cert.digest(), Some(cert)))
                .collect(),
        )
        .expect("Storage is ok");
    let certs_back = authority_state
        .database
        .get_pending_digests()
        .expect("DB should be there");
    assert_eq!(num_certs, certs_back.len());

    // In the time we are waiting the execution logic re-executes the
    // transactions and therefore we have no certificate left pending at the end.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // get back the certificates
    let certs_back = authority_state
        .database
        .get_pending_digests()
        .expect("DB should be there");
    assert_eq!(0, certs_back.len());
}

#[tokio::test]
async fn test_parent_cert_exec() {
    telemetry_subscribers::init_for_testing();

    let (addr1, key1): (_, AccountKeyPair) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let gas_object2 = Object::with_owner_for_testing(addr1);
    let (aggregator, authorities) =
        init_local_authorities(4, vec![gas_object1.clone(), gas_object2.clone()]).await;
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| &aggregator.authority_clients[&a.name])
        .collect();

    let framework_obj_ref = genesis::get_framework_object_ref();

    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(authority_clients[0], gas_object1.id()).await;
    let tx1 = crate_object_move_transaction(addr1, &key1, addr1, 100, framework_obj_ref, gas_ref_1);

    // create an object and execute the cert on 3 authorities
    do_transaction(authority_clients[0], &tx1).await;
    do_transaction(authority_clients[1], &tx1).await;
    do_transaction(authority_clients[2], &tx1).await;
    let cert1 = extract_cert(&authority_clients, &aggregator.committee, tx1.digest()).await;

    do_cert(authority_clients[0], &cert1).await;
    do_cert(authority_clients[1], &cert1).await;
    let effects1 = do_cert(authority_clients[2], &cert1).await;
    info!(digest = ?tx1.digest(), "cert1 finished");

    // now create a tx to transfer that object (only on 3 authorities), and then execute it on one
    // authority only.
    let (addr2, _): (_, AccountKeyPair) = get_key_pair();

    let tx2 = transfer_object_move_transaction(
        addr1,
        &key1,
        addr2,
        effects1.created[0].0,
        framework_obj_ref,
        effects1.gas_object.0,
    );

    do_transaction(authority_clients[0], &tx2).await;
    do_transaction(authority_clients[1], &tx2).await;
    do_transaction(authority_clients[2], &tx2).await;
    let cert2 = extract_cert(&authority_clients, &aggregator.committee, tx2.digest()).await;
    do_cert(authority_clients[0], &cert2).await;
    info!(digest = ?tx2.digest(), "cert2 finished");

    // the 4th authority has never heard of either of these transactions. Tell it to execute the
    // cert and verify that it is able to fetch parents and apply.
    let active_state = Arc::new(
        ActiveAuthority::new_with_ephemeral_storage_for_test(
            authorities[3].clone(),
            aggregator.clone(),
        )
        .unwrap(),
    );

    let batch_state = authorities[3].clone();
    tokio::task::spawn(async move {
        batch_state
            .run_batch_service(1, Duration::from_secs(1))
            .await
    });
    active_state.clone().spawn_execute_process().await;

    authorities[3]
        .database
        .add_pending_certificates(vec![(*tx2.digest(), None)])
        .unwrap();

    wait_for_tx(*tx2.digest(), authorities[3].clone()).await;

    // verify it has the cert.
    authority_clients[3]
        .handle_transaction_info_request((*tx2.digest()).into())
        .await
        .unwrap()
        .signed_effects
        .unwrap();
}
