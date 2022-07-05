// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_active::ActiveAuthority, checkpoints::checkpoint_tests::TestSetup};

use crate::authority_active::checkpoint_driver::CheckpointMetrics;
use std::sync::Arc;
use std::time::Duration;
use sui_types::messages::ExecutionStatus;

use crate::checkpoints::checkpoint_tests::checkpoint_tests_setup;

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
            ActiveAuthority::new_with_ephemeral_storage(inner_state.authority.clone(), inner_agg)
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
                effects.effects.status,
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
        .get_pending_certificates()
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
                ActiveAuthority::new_with_ephemeral_storage(
                    inner_state.authority.clone(),
                    inner_agg,
                    Default::default(),
                )
                .unwrap(),
            );

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
                effects.effects.status,
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
        .get_pending_certificates()
        .expect("DB should be there");
    assert_eq!(num_certs, certs_back.len());

    // In the time we are waiting the execution logic re-executes the
    // transactions and therefore we have no certificate left pending at the end.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // get back the certificates
    let certs_back = authority_state
        .database
        .get_pending_certificates()
        .expect("DB should be there");
    assert_eq!(0, certs_back.len());
}
