// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::register_debug_fatal_handler;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use sui_macros::register_fail_point_arg;
use sui_macros::sim_test;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::authenticator_state::get_authenticator_state_obj_initial_shared_version;
use sui_types::base_types::AuthorityName;
use sui_types::transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;
use tokio::time::sleep;
use tracing::info;

#[sim_test]
async fn basic_checkpoints_integration_test() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    let digest = *tx.digest();
    test_cluster.execute_transaction(tx).await;

    for _ in 0..600 {
        let all_included = test_cluster
            .swarm
            .validator_node_handles()
            .into_iter()
            .all(|handle| {
                handle.with(|node| {
                    node.state()
                        .epoch_store_for_testing()
                        .is_transaction_executed_in_checkpoint(&digest)
                        .unwrap()
                })
            });
        if all_included {
            // success
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("Did not include transaction in checkpoint in 60 seconds");
}

#[sim_test]
async fn test_checkpoint_split_brain() {
    #[cfg(msim)]
    {
        // this test intentionally halts the network by causing a fork, so we cannot panic on
        // loss of liveness
        use sui_core::authority::{CheckpointTimeoutConfig, init_checkpoint_timeout_config};
        init_checkpoint_timeout_config(CheckpointTimeoutConfig {
            warning_timeout: Duration::from_secs(2),
            panic_timeout: None,
        });
    }

    let committee_size = 9;
    // count number of nodes that have reached split brain condition
    let count_split_brain_nodes: Arc<Mutex<AtomicUsize>> = Default::default();
    let count_clone = count_split_brain_nodes.clone();

    register_debug_fatal_handler!(
        "Split brain detected in checkpoint signature aggregation",
        move || {
            let counter = count_clone.lock().unwrap();
            counter.fetch_add(1, Ordering::Relaxed);
        }
    );

    register_fail_point_arg("simulate_fork_during_execution", || {
        Some((
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::<
                AuthorityName,
            >::new())),
            true, // full_halt
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::<
                String,
                String,
            >::new())),
            1.0f32, // fork_probability
        ))
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(committee_size)
        .build()
        .await;

    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(tx)
        .await
        .ok();

    // provide enough time for validators to detect split brain
    tokio::time::sleep(Duration::from_secs(20)).await;

    // all honest validators should eventually detect a split brain
    let final_count = count_split_brain_nodes.lock().unwrap();
    assert!(final_count.load(Ordering::Relaxed) >= 1);
}

#[sim_test]
async fn test_checkpoint_timestamps_non_decreasing() {
    let epoch_duration_ms = 10_000; // 10 seconds
    let num_epochs_to_run = 3;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(epoch_duration_ms)
        .disable_fullnode_pruning()
        .build()
        .await;

    sleep(Duration::from_millis(
        epoch_duration_ms * num_epochs_to_run + epoch_duration_ms / 2,
    ))
    .await;

    // Retrieve checkpoints and verify timestamps from the first full node.
    let full_node = test_cluster
        .swarm
        .fullnodes()
        .next()
        .expect("No full node is found");

    let checkpoint_store = full_node
        .get_node_handle()
        .unwrap()
        .state()
        .checkpoint_store
        .clone();

    let highest_executed_checkpoint = checkpoint_store
        .get_highest_executed_checkpoint()
        .expect("Failed to get highest executed checkpoint")
        .expect("No executed checkpoints found in store");

    assert!(
        highest_executed_checkpoint.epoch() > 0,
        "Test did not run long enough to cross epochs"
    );

    let mut current_seq = *highest_executed_checkpoint.sequence_number();
    let mut prev_timestamp = highest_executed_checkpoint.timestamp();
    let mut checkpoints_checked = 0;

    // Iterate backwards from the highest checkpoint
    loop {
        if current_seq == 0 {
            info!("Reached checkpoint 0.");
            break;
        }
        current_seq -= 1;

        // Fetch the previous digest to continue iteration
        let current_checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(current_seq)
            .expect("DB error getting current checkpoint")
            .unwrap_or_else(|| panic!("checkpoint missing for seq {}", current_seq));
        let current_timestamp = current_checkpoint.timestamp();
        assert!(
            current_timestamp <= prev_timestamp,
            "Timestamp decreased! current seq {}, {:?} vs {:?}",
            current_seq,
            current_timestamp,
            prev_timestamp,
        );
        prev_timestamp = current_timestamp;
        checkpoints_checked += 1;
    }

    assert!(checkpoints_checked > 0, "Test created only 1 checkpoint",);
}

#[sim_test]
async fn test_checkpoint_fork_detection_storage() {
    use sui_types::messages_checkpoint::CheckpointDigest;

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .build()
        .await;

    // Get the first validator for testing
    let validator_handle = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .next()
        .expect("No validator found");

    // Test 1: Basic fork detection storage functionality
    validator_handle.with(|node| {
        let checkpoint_store = node.state().checkpoint_store.clone();
        let fork_seq = 42;
        let fork_digest = CheckpointDigest::random();

        assert!(
            checkpoint_store
                .get_checkpoint_fork_detected()
                .unwrap()
                .is_none()
        );

        checkpoint_store
            .record_checkpoint_fork_detected(fork_seq, fork_digest)
            .expect("Failed to record checkpoint fork");

        let retrieved = checkpoint_store.get_checkpoint_fork_detected().unwrap();
        assert!(retrieved.is_some());
        let (retrieved_seq, retrieved_digest) = retrieved.unwrap();
        assert_eq!(retrieved_seq, fork_seq);
        assert_eq!(retrieved_digest, fork_digest);

        checkpoint_store.clear_checkpoint_fork_detected().unwrap();
        let retrieved_after_clear = checkpoint_store.get_checkpoint_fork_detected().unwrap();
        assert!(
            retrieved_after_clear.is_none(),
            "Fork state should be cleared"
        );
    });
}

#[sim_test]
async fn test_checkpoint_contents_v2_alias_versions() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(2)
        .build()
        .await;

    let (account, gas_objects) = test_cluster.wallet.get_one_account().await.unwrap();
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    assert!(
        gas_objects.len() >= 3,
        "require at least three gas objects for this test"
    );

    let authenticator_state_initial_shared_version = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .next()
        .unwrap()
        .with(|node| {
            get_authenticator_state_obj_initial_shared_version(
                node.state().get_object_store().as_ref(),
            )
            .expect("failed to get authenticator state object")
            .expect("authenticator state object should exist")
        });

    // Submit two tx in a soft bundle. First one calls `init_aliases`.
    // The soft bundle forces checkpoint output to contain out-of-order alias
    // versions: even though `init_aliases` changes the alias config version,
    // the second tx (just a dummy transfer) should still report that it was
    // verified using the old `None` version of the alias config.
    let init_aliases_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account, gas_objects[0], gas_price)
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "authenticator_state",
                    "init_aliases",
                    vec![CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                        initial_shared_version: authenticator_state_initial_shared_version,
                        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                    })],
                )
                .build(),
        )
        .await;
    let init_aliases_digest = *init_aliases_tx.digest();
    let transfer_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account, gas_objects[1], gas_price)
                .programmable(ProgrammableTransaction {
                    inputs: vec![
                        // Add dependency on shared object to force consensus.
                        CallArg::Object(ObjectArg::SharedObject {
                            id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                            initial_shared_version: authenticator_state_initial_shared_version,
                            mutability: sui_types::transaction::SharedObjectMutability::Immutable,
                        }),
                        CallArg::Pure(bcs::to_bytes(&account).unwrap()),
                    ],
                    commands: vec![Command::TransferObjects(
                        vec![Argument::GasCoin],
                        Argument::Input(1),
                    )],
                })
                .build(),
        )
        .await;
    let transfer_digest = *transfer_tx.digest();

    let mut client = test_cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .next()
        .unwrap()
        .1
        .authority_client()
        .client_for_testing();
    let request = sui_types::messages_grpc::RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&init_aliases_tx).unwrap().into(),
            bcs::to_bytes(&transfer_tx).unwrap().into(),
        ],
        submit_type: sui_types::messages_grpc::SubmitTxType::SoftBundle.into(),
    };
    let result = client
        .submit_transaction(request)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(result.results.len(), 2);
    assert!(matches!(
        result.results[0].inner,
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_))
    ));
    assert!(matches!(
        result.results[1].inner,
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_))
    ));

    let validator_handle = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .next()
        .expect("No validator found");

    let mut checkpoint_seq = None;
    for _ in 0..600 {
        checkpoint_seq = validator_handle.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .get_transaction_checkpoint(&init_aliases_digest)
                .unwrap()
        });
        if checkpoint_seq.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let checkpoint_seq =
        checkpoint_seq.expect("Did not include transaction in checkpoint in 60 seconds");

    let original_alias_versions = validator_handle.with(|node| {
        let checkpoint_store = node.state().checkpoint_store.clone();

        let checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(checkpoint_seq)
            .unwrap()
            .expect("Checkpoint not found");

        let checkpoint_contents = checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .unwrap()
            .expect("Checkpoint contents not found");

        let contents_view = checkpoint_contents.inner();

        // Find both of the tx. They must appear in the same checkpoint.
        let (init_idx, transfer_idx) = contents_view.digests_iter().enumerate().fold(
            (None, None),
            |(init, transfer), (idx, digest)| {
                let new_init = if digest.transaction == init_aliases_digest {
                    Some(idx)
                } else {
                    init
                };
                let new_transfer = if digest.transaction == transfer_digest {
                    Some(idx)
                } else {
                    transfer
                };
                (new_init, new_transfer)
            },
        );

        let init_idx = init_idx.expect("init_aliases transaction not found in checkpoint contents");
        let transfer_idx =
            transfer_idx.expect("transfer transaction not found in checkpoint contents");
        assert!(init_idx < transfer_idx, "init_aliases transaction must appear before transfer transaction in checkpoint contents, got init_idx: {}, transfer_idx: {}", init_idx, transfer_idx);

        let init_signatures = contents_view
            .user_signatures(init_idx)
            .expect("init_aliases transaction signatures not found");
        let transfer_signatures = contents_view
            .user_signatures(transfer_idx)
            .expect("transfer transaction signatures not found");

        // Make sure they both were verified using the same alias config versions, even though
        // the first tx changed the alias config.
        assert_eq!(
            init_signatures.len(),
            transfer_signatures.len(),
            "Both transactions should have the same number of signatures"
        );
        for ((_, init_version), (_, transfer_version)) in
            init_signatures.iter().zip(transfer_signatures.iter())
        {
            assert_eq!(
                init_version, transfer_version,
                "Alias version mismatch: init {:?}, transfer {:?}",
                init_version, transfer_version
            );
        }

        // Return the original alias versions for later comparison
        init_signatures.iter().map(|(_, v)| *v).collect::<Vec<_>>()
    });

    // Submit a new transaction after init_aliases has been executed.
    // This should use the new alias version (different from the soft bundle transactions).
    let post_init_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account, gas_objects[2], gas_price)
                .transfer_sui(None, account)
                .build(),
        )
        .await;
    let post_init_digest = *post_init_tx.digest();
    let result = client
        .submit_transaction(sui_types::messages_grpc::RawSubmitTxRequest {
            transactions: vec![bcs::to_bytes(&post_init_tx).unwrap().into()],
            submit_type: sui_types::messages_grpc::SubmitTxType::Default.into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(result.results.len(), 1);
    assert!(matches!(
        result.results[0].inner,
        Some(sui_types::messages_grpc::RawValidatorSubmitStatus::Submitted(_))
    ));

    // Wait for the transaction to be included in a checkpoint
    let mut post_init_checkpoint_seq = None;
    for _ in 0..600 {
        post_init_checkpoint_seq = validator_handle.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .get_transaction_checkpoint(&post_init_digest)
                .unwrap()
        });
        if post_init_checkpoint_seq.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let post_init_checkpoint_seq = post_init_checkpoint_seq
        .expect("Did not include post-init transaction in checkpoint in 60 seconds");

    validator_handle.with(|node| {
        let checkpoint_store = node.state().checkpoint_store.clone();

        let checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(post_init_checkpoint_seq)
            .unwrap()
            .expect("Checkpoint not found");

        let checkpoint_contents = checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .unwrap()
            .expect("Checkpoint contents not found");

        let contents_view = checkpoint_contents.inner();

        let post_init_idx = contents_view
            .digests_iter()
            .enumerate()
            .find(|(_, digest)| digest.transaction == post_init_digest)
            .map(|(idx, _)| idx)
            .expect("post-init transaction not found in checkpoint contents");

        let post_init_signatures = contents_view
            .user_signatures(post_init_idx)
            .expect("post-init transaction signatures not found");

        // Verify that the post-init transaction uses a different alias version
        assert_eq!(
            post_init_signatures.len(),
            original_alias_versions.len(),
            "Both transactions should have the same number of signatures"
        );
        for ((_, post_init_version), original_version) in
            post_init_signatures.iter().zip(original_alias_versions.iter())
        {
            assert_ne!(
                post_init_version, original_version,
                "Post-init transaction should use different alias version than soft bundle. post_init: {:?}, original: {:?}",
                post_init_version, original_version
            );
        }
    });
}
