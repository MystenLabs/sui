// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_active::ActiveAuthority;
use crate::authority_aggregator::authority_aggregator_tests::{
    crate_object_move_transaction, do_cert, do_transaction, extract_cert, get_latest_ref,
    init_local_authorities, transfer_object_move_transaction,
};
use crate::test_utils::wait_for_tx;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use itertools::Itertools;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::messages::VerifiedCertificate;
use sui_types::object::Object;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;
use tracing::info;

#[allow(dead_code)]
async fn wait_for_certs(
    stream: &mut UnboundedReceiver<VerifiedCertificate>,
    certs: &Vec<VerifiedCertificate>,
) {
    if certs.is_empty() {
        if timeout(Duration::from_secs(30), stream.recv())
            .await
            .is_err()
        {
            return;
        } else {
            panic!("Should not receive certificate!");
        }
    }
    let mut certs: BTreeSet<TransactionDigest> = certs.iter().map(|c| *c.digest()).collect();
    while !certs.is_empty() {
        match timeout(Duration::from_secs(5), stream.recv()).await {
            Err(_) => panic!("Timed out waiting for next certificate!"),
            Ok(None) => panic!("Next certificate channel closed!"),
            Ok(Some(cert)) => {
                println!("Found cert {:?}", cert.digest());
                certs.remove(cert.digest())
            }
        };
    }
}

/*
TODO: Re-enable after we have checkpoint v2.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_notify_ready_certificates() {
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
    let mut ready_certificates_stream = authority_state.ready_certificates_stream().await.unwrap();

    // TODO: duplicated with checkpoint_driver/tests.rs
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
            .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
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
                effects.data().status,
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

    // Clear effects so their executions will happen below.
    authority_state
        .database
        .perpetual_tables
        .effects
        .clear()
        .expect("Clearing effects failed!");

    // Insert the certificates
    authority_state
        .add_pending_certificates(certs.clone())
        .await
        .expect("Storage is ok");

    tokio::task::yield_now().await;

    // Wait to get back the certificates
    wait_for_certs(&mut ready_certificates_stream, &certs).await;

    // Should have no certificate any more.
    wait_for_certs(&mut ready_certificates_stream, &vec![]).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_full() {
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
        let _active_handle = tokio::task::spawn(async move {
            let active_state = Arc::new(
                ActiveAuthority::new_with_ephemeral_storage_for_test(
                    inner_state.authority.clone(),
                    inner_agg,
                )
                .unwrap(),
            );
            let batch_state = inner_state.authority.clone();
            tokio::task::spawn(async move {
                batch_state
                    .run_batch_service(1, Duration::from_secs(1))
                    .await
            });
            active_state.clone().spawn_execute_process().await;
            active_state
                .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
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
                effects.data().status,
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
    authority_state
        .add_pending_certificates(certs.clone())
        .await
        .expect("Storage is ok");

    // Wait for execution.
    for cert in certs {
        wait_for_tx(*cert.digest(), authority_state.clone()).await;
    }
}

 */

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_transaction_manager() {
    telemetry_subscribers::init_for_testing();

    let (addr1, key1): (_, AccountKeyPair) = get_key_pair();
    let gas_objects = vec![0..100]
        .iter()
        .map(|_| Object::with_owner_for_testing(addr1))
        .collect_vec();
    let (aggregator, authorities, framework_obj_ref) =
        init_local_authorities(4, gas_objects.clone()).await;
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| &aggregator.authority_clients[&a.name])
        .collect();

    // Make a schedule of transactions
    let gas_ref_0 = get_latest_ref(authority_clients[0], gas_objects[0].id()).await;
    let tx1 = crate_object_move_transaction(addr1, &key1, addr1, 100, framework_obj_ref, gas_ref_0);

    // create an object and execute the cert on 3 authorities
    do_transaction(authority_clients[0], &tx1).await;
    do_transaction(authority_clients[1], &tx1).await;
    do_transaction(authority_clients[2], &tx1).await;
    let cert1 = extract_cert(&authority_clients, &aggregator.committee, tx1.digest())
        .await
        .verify(&aggregator.committee)
        .unwrap();

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
    let cert2 = extract_cert(&authority_clients, &aggregator.committee, tx2.digest())
        .await
        .verify(&aggregator.committee)
        .unwrap();
    do_cert(authority_clients[0], &cert2).await;
    info!(digest = ?tx2.digest(), "cert2 finished");

    // the 4th authority has never heard of either of these transactions. Tell it to execute the
    // cert, sends it the missing dependency and verify that it is able to fetch parents and apply.
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

    // Basic test: add certs out of dependency order. They should still be executed.
    authorities[3]
        .add_pending_certificates(vec![cert2.clone()])
        .await
        .unwrap();
    authorities[3]
        .add_pending_certificates(vec![cert1.clone()])
        .await
        .unwrap();

    wait_for_tx(*tx2.digest(), authorities[3].clone()).await;
    // verify it has the effect.
    authority_clients[3]
        .handle_transaction_info_request((*tx2.digest()).into())
        .await
        .unwrap()
        .signed_effects
        .unwrap();

    // TODO: more test cases.
}
