// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use rand::{rngs::StdRng, SeedableRng};
use std::collections::HashSet;
use std::sync::Arc;
use sui_core::authority_active::checkpoint_driver::CheckpointMetrics;
use sui_core::{
    authority::AuthorityState,
    authority_active::{checkpoint_driver::CheckpointProcessControl, ActiveAuthority},
    authority_aggregator::AuthorityAggregator,
    authority_client::NetworkAuthorityClient,
};
use sui_node::SuiNode;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest},
    crypto::get_key_pair_from_rng,
    messages::{CallArg, ExecutionStatus, ObjectArg, Transaction},
};
use test_utils::transaction::publish_counter_package;
use test_utils::{
    authority::{
        spawn_test_authorities, submit_shared_object_transaction, test_authority_aggregator,
        test_authority_configs,
    },
    messages::{move_transaction, test_transactions},
    objects::test_gas_objects,
};
use tokio::time::{sleep, Duration, Instant};
use typed_store::Map;

/// Helper function determining whether the checkpoint store of an authority contains the input
/// transactions' digests.
fn transactions_in_checkpoint(authority: &AuthorityState) -> HashSet<TransactionDigest> {
    let checkpoints_store = authority.checkpoints().unwrap();

    // Get all transactions in the first 10 checkpoints.
    (0..10)
        .filter_map(|checkpoint_sequence| {
            checkpoints_store
                .lock()
                .checkpoint_contents
                .get(&checkpoint_sequence)
                .unwrap()
        })
        .flat_map(|x| x.iter().map(|tx| tx.transaction).collect::<HashSet<_>>())
        .collect::<HashSet<_>>()
}

async fn spawn_checkpoint_processes(
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
    handles: &[SuiNode],
) {
    // Start active part of each authority.
    for authority in &handles {
        let state = authority.state().clone();
        let inner_agg = aggregator.clone();
        let active_state =
            Arc::new(ActiveAuthority::new_with_ephemeral_storage(state, inner_agg).unwrap());
        let checkpoint_process_control = CheckpointProcessControl {
            long_pause_between_checkpoints: Duration::from_millis(10),
            ..CheckpointProcessControl::default()
        };
        let _active_authority_handle = active_state
            .spawn_checkpoint_process_with_config(
                checkpoint_process_control,
                CheckpointMetrics::new_for_tests(),
            )
            .await;
    }
}

async fn execute_transactions(
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
    transactions: &[Transaction],
) {
    for transaction in transactions {
        let (_, effects) = aggregator
            .clone()
            .execute_transaction(transaction)
            .await
            .unwrap();

        // If this check fails the transactions will not be included in the checkpoint.
        assert!(matches!(
            effects.effects.status,
            ExecutionStatus::Success { .. }
        ));

        // Add some delay between transactions
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn wait_for_advance_to_next_checkpoint(
    handles: &[SuiNode],
    transaction_digests: &HashSet<TransactionDigest>,
) {
    // Wait for the transactions to be executed and end up in a checkpoint.
    let mut cnt = 0;
    loop {
        // Ensure all submitted transactions are in the checkpoint.
        let ok = handles
            .iter()
            .map(|authority| transactions_in_checkpoint(&authority.state()))
            .all(|digests| digests.is_superset(&transaction_digests));

        match ok {
            true => break,
            false => tokio::time::sleep(Duration::from_secs(1)).await,
        }
        cnt += 1;
        assert!(cnt <= 20);
    }

    // Ensure all authorities moved to the next checkpoint sequence number.
    let ok = handles
        .iter()
        .map(|authority| {
            authority
                .state()
                .checkpoints()
                .unwrap()
                .lock()
                .get_locals()
                .next_checkpoint
        })
        .all(|sequence| sequence >= 1);
    assert!(ok);

    for node in handles {
        for digest in transaction_digests.iter() {
            assert!(node
                .state()
                .check_tx_already_executed(digest)
                .await
                .unwrap()
                .is_some());
        }
    }
}

#[tokio::test]
async fn sequence_fragments() {
    // Spawn a quorum of authorities.
    let configs = test_authority_configs();
    let mut handles = spawn_test_authorities(vec![], &configs).await;
    let committee = &handles[0].state().clone_committee();

    // Get checkpoint proposals.
    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let transactions = [(1, t1), (2, t2), (3, t3)];
    let next_sequence_number = (transactions.len() + 1) as u64;

    let mut proposals: Vec<_> = handles
        .iter_mut()
        .map(|handle| {
            let checkpoints_store = handle.state().checkpoints().unwrap();
            checkpoints_store
                .lock()
                .handle_internal_batch(next_sequence_number, &transactions)
                .unwrap();
            let proposal = checkpoints_store
                .lock()
                .set_proposal(committee.epoch)
                .unwrap();
            proposal
        })
        .collect();

    // Ensure the are no fragments in the checkpoint store at this time.
    for handle in &handles {
        let status = handle
            .state()
            .checkpoints()
            .unwrap()
            .lock()
            .fragments
            .iter()
            .skip_to_last()
            .next();
        assert!(status.is_none());
    }

    // Make a checkpoint fragment and sequence it.
    let p1 = proposals.pop().unwrap();
    let p2 = proposals.pop().unwrap();
    let fragment = p1.fragment_with(&p2);

    for handle in handles.iter_mut() {
        let _response = handle
            .state()
            .checkpoints()
            .unwrap()
            .lock()
            .submit_local_fragment_to_consensus(&fragment, committee);
    }

    // Wait until all validators sequence and process the fragment.
    loop {
        let ok = handles.iter().all(|handle| {
            handle
                .state()
                .checkpoints()
                .unwrap()
                .lock()
                .fragments
                .iter()
                .next()
                .is_some()
        });
        if ok {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn end_to_end() {
    telemetry_subscribers::init_for_testing();
    // Make a few test transactions.
    let total_transactions = 3;
    let mut rng = StdRng::from_seed([0; 32]);
    let keys = (0..total_transactions).map(|_| get_key_pair_from_rng(&mut rng).1);
    let (transactions, input_objects) = test_transactions(keys);
    let transaction_digests: HashSet<_> = transactions.iter().map(|x| *x.digest()).collect();

    // Spawn a quorum of authorities.
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(input_objects, &configs).await;

    // Make an authority's aggregator.
    let aggregator = test_authority_aggregator(&configs);

    spawn_checkpoint_processes(&aggregator, &handles).await;

    execute_transactions(&aggregator, &transactions).await;

    wait_for_advance_to_next_checkpoint(&handles, &transaction_digests).await;
}

#[tokio::test]
async fn checkpoint_with_shared_objects() {
    telemetry_subscribers::init_for_testing();

    // Get some gas objects to submit shared-objects transactions.
    let mut gas_objects = test_gas_objects();

    // Make a few test transactions.
    let total_transactions = 3;
    let mut rng = StdRng::from_seed([0; 32]);
    let keys = (0..total_transactions).map(|_| get_key_pair_from_rng(&mut rng).1);
    let (transactions, input_objects) = test_transactions(keys);

    // Spawn a quorum of authorities.
    let configs = test_authority_configs();
    let initialization_objects = input_objects.into_iter().chain(gas_objects.iter().cloned());
    let handles = spawn_test_authorities(initialization_objects, &configs).await;

    // Make an authority's aggregator.
    let aggregator = test_authority_aggregator(&configs);

    spawn_checkpoint_processes(&aggregator, &handles).await;

    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    let gas = gas_objects.pop().unwrap();
    let package_ref = publish_counter_package(gas, configs.validator_set()).await;

    // Make a transaction to create a counter.
    tokio::task::yield_now().await;
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let (_, effects) = aggregator
        .execute_transaction(&create_counter_transaction)
        .await
        .unwrap();
    assert!(matches!(
        effects.effects.status,
        ExecutionStatus::Success { .. }
    ));
    let ((counter_id, _, _), _) = effects.effects.created[0];

    // We can finally make a valid shared-object transaction (incrementing the counter).
    tokio::task::yield_now().await;
    let increment_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(ObjectArg::SharedObject(counter_id))],
    );
    let replies = submit_shared_object_transaction(
        increment_counter_transaction.clone(),
        configs.validator_set(),
    )
    .await;
    for reply in replies {
        match reply {
            Ok(info) => {
                let effects = info.signed_effects.unwrap().effects;
                assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            }
            Err(error) => panic!("{error}"),
        }
    }

    // Now send a few single-writer transactions.
    execute_transactions(&aggregator, transactions).await;

    // Record the transactions digests we expect to see in the checkpoint. Note that there is also
    // an extra transaction to register the move module that we don't consider here.
    let mut transaction_digests: HashSet<_> = transactions.iter().map(|x| *x.digest()).collect();
    transaction_digests.insert(*create_counter_transaction.digest());
    transaction_digests.insert(*increment_counter_transaction.digest());

    wait_for_advance_to_next_checkpoint(&handles, &transaction_digests).await;
}

// Check that a disconnected validator syncs all certs in past checkpoints as soon as it is able.
// This test should fail if sync_checkpoint_certs in checkpoint_driver/mod.rs is commented out.
#[tokio::test]
async fn checkpoint_catchup() {
    telemetry_subscribers::init_for_testing();
    // Make a few test transactions.
    let total_transactions = 3;
    let mut rng = StdRng::from_seed([0; 32]);
    let keys = (0..total_transactions).map(|_| get_key_pair_from_rng(&mut rng).1);
    let (transactions, input_objects) = test_transactions(keys);
    let transaction_digests: HashSet<_> = transactions.iter().map(|x| *x.digest()).collect();

    // Spawn a quorum of authorities.
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(input_objects, &configs).await;

    // Make an authority's aggregator.
    let aggregator = test_authority_aggregator(&configs);

    let (first, rest) = handles[..].split_at(1);

    // halt first validator so it can't process txes
    first[0].state().halt_validator_for_testing();

    spawn_checkpoint_processes(&aggregator, rest).await;

    execute_transactions(&aggregator, &transactions).await;

    // Wait until all but one validator is caught up.
    wait_for_advance_to_next_checkpoint(rest, &transaction_digests).await;

    // now start the checkpoint process on the first validator and wait for it to sync.
    first[0].state().unhalt_validator_for_testing();
    spawn_checkpoint_processes(&aggregator, first).await;
    wait_for_advance_to_next_checkpoint(first, &transaction_digests).await;
}
