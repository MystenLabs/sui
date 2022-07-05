// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority_active::{checkpoint_driver::CheckpointProcessControl, ActiveAuthority},
    authority_client::LocalAuthorityClient,
    checkpoints::checkpoint_tests::TestSetup, safe_client::SafeClient,
};

use crate::authority_active::checkpoint_driver::CheckpointMetrics;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use sui_types::messages::ExecutionStatus;

use crate::checkpoints::checkpoint_tests::checkpoint_tests_setup;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_active_flow_happy_path() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

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
            println!("Effects: {:?}", effects.effects.digest());

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }
    });

    // Wait for all the sending to happen.
    _end_of_sending_join.await.expect("all ok");

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_secs(20 * 60)).await;

    let mut value_set = BTreeSet::new();
    for a in authorities {
        let next_checkpoint_sequence = a
            .authority
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .next_checkpoint();
        // TODO: This check is not very meaningful after we allowed empty checkpoints.
        // What we want to check is probably the number of non-empty checkpoints.
        assert!(
            next_checkpoint_sequence >= 2,
            "Expected {} >= 2",
            next_checkpoint_sequence
        );
        value_set.insert(next_checkpoint_sequence);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_active_flow_crash_client_with_gossip() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(500), false).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

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

            println!("Start active execution process.");
            active_state.clone().spawn_execute_process().await;

            // Spin the checkpoint service.
            active_state
                .spawn_checkpoint_process_with_config(Default::default(), CheckpointMetrics::new_for_tests())
                .await;
        });
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        while let Some(t) = transactions.pop() {
            // Get a cert
            let new_certificate = sender_aggregator
                .process_transaction(t.clone())
                .await
                .expect("Unexpected crash");

            // Send it only to 1 random node
            let sample_authority = sender_aggregator.committee.sample();
            let client: SafeClient<LocalAuthorityClient> =
                sender_aggregator.authority_clients[sample_authority].clone();
            let _response = client
                .handle_certificate(new_certificate)
                .await
                .expect("Problem processing certificate");

            // Check whether this is a success?
            assert!(matches!(
                _response.signed_effects.unwrap().effects.status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }
    });

    // Wait for all the sending to happen.
    _end_of_sending_join.await.expect("all ok");

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_secs(180 * 60)).await;

    let mut value_set = BTreeSet::new();
    for a in authorities {
        let next_checkpoint_sequence = a
            .authority
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .next_checkpoint();
        // TODO: This check is not very meaningful after we allowed empty checkpoints.
        // What we want to check is probably the number of non-empty checkpoints.
        assert!(
            next_checkpoint_sequence > 1,
            "Expected {} > 1",
            next_checkpoint_sequence
        );
        value_set.insert(next_checkpoint_sequence);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_active_flow_crash_client_no_gossip() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), false).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

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

            println!("Start active execution process.");
            active_state.clone().spawn_execute_process().await;

            // Spin the gossip service.
            active_state
                .spawn_checkpoint_process_with_config(CheckpointProcessControl::default()), CheckpointMetrics::new_for_tests())
                .await;
        });
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        while let Some(t) = transactions.pop() {
            // Get a cert
            let new_certificate = sender_aggregator
                .process_transaction(t.clone())
                .await
                .expect("Unexpected crash");

            // Send it only to 1 random node
            let sample_authority = sender_aggregator.committee.sample();
            let client: SafeClient<LocalAuthorityClient> =
                sender_aggregator.authority_clients[sample_authority].clone();
            let _response = client
                .handle_certificate(new_certificate)
                .await
                .expect("Problem processing certificate");

            // Check whether this is a success?
            assert!(matches!(
                _response.signed_effects.unwrap().effects.status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }
    });

    // Wait for all the sending to happen.
    _end_of_sending_join.await.expect("all ok");

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_secs(10 * 60)).await;

    let mut value_set = BTreeSet::new();
    for a in authorities {
        let next_checkpoint_sequence = a
            .authority
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .next_checkpoint();
        // TODO: This check is not very meaningful after we allowed empty checkpoints.
        // What we want to check is probably the number of non-empty checkpoints.
        assert!(
            next_checkpoint_sequence > 1,
            "Expected {} > 1",
            next_checkpoint_sequence
        );
        value_set.insert(next_checkpoint_sequence);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_empty_checkpoint() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(0, Duration::from_millis(200), false).await;

    let TestSetup {
        committee: _committee,
        authorities,
        transactions: _,
        aggregator,
    } = setup;

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

            // Spawn the checkpointing service.
            active_state
                .spawn_checkpoint_process_with_config(CheckpointProcessControl::default(), CheckpointMetrics::new_for_tests())
                .await;
        });
    }

    // Wait for long enough to have generated some checkpoint.
    tokio::time::sleep(Duration::from_secs(10 * 60)).await;

    for a in authorities {
        let next_checkpoint_sequence = a
            .authority
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .next_checkpoint();
        assert!(next_checkpoint_sequence > 0)
    }
}
