// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use sui_types::base_types::ExecutionDigests;
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
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
