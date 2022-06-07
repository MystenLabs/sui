// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use rand::{rngs::StdRng, SeedableRng};
use sui_core::authority_active::{checkpoint_driver::CheckpointProcessControl, ActiveAuthority};
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::{base_types::ExecutionDigests, messages::ExecutionStatus};
use test_utils::{
    authority::{spawn_test_authorities, test_authority_aggregator, test_authority_configs},
    messages::test_transactions,
};
use tokio::time::sleep;
use tokio::time::Duration;
use typed_store::Map;

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
            let proposal = checkpoints_store.lock().set_proposal().unwrap();
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
            .handle_receive_fragment(&fragment, committee);
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
    // Make a few test transactions.
    let mut rng = StdRng::from_seed([0; 32]);
    let keys = (0..3).map(|_| get_key_pair_from_rng(&mut rng).1);
    let (mut transactions, input_objects) = test_transactions(keys);

    // Spawn a quorum of authorities.
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(input_objects, &configs).await;

    // Make an authority's aggregator.
    let aggregator = test_authority_aggregator(&configs);

    // Start active part of each authority.
    for authority in &handles {
        let state = authority.state().clone();
        let clients = aggregator.clone_inner_clients();
        let _active_authority_handle = tokio::spawn(async move {
            let active_state = ActiveAuthority::new(state, clients).unwrap();
            let checkpoint_process_control = CheckpointProcessControl {
                long_pause_between_checkpoints: Duration::from_millis(10),
                ..CheckpointProcessControl::default()
            };
            active_state
                .spawn_active_processes(true, true, checkpoint_process_control)
                .await
        });
    }

    // Send the transactions for execution.
    while let Some(transaction) = transactions.pop() {
        let (_, effects) = aggregator
            .clone()
            .execute_transaction(&transaction)
            .await
            .unwrap();

        // If this check fails the transactions will not be included in the checkpoint.
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

        // Add some delay between transactions
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Wait for the transactions to be executed and end up in a checkpoint.
    loop {
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
        if ok {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
