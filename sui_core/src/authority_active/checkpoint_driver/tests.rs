// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::{checkpoint_tests::TestSetup, proposal::CheckpointProposal};

use std::{collections::HashSet, time::Duration};
use sui_types::{
    messages::ExecutionStatus,
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpoint, CheckpointRequest,
    },
};

use crate::checkpoints::checkpoint_tests::checkpoint_tests_setup;

use crate::authority_client::AuthorityAPI;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_active_flow() {
    let setup = checkpoint_tests_setup(100, Duration::from_millis(200)).await;

    let TestSetup {
        committee,
        authorities: _authorities,
        mut transactions,
        aggregator,
    } = setup;

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            println!("Execute at {:?}", tokio::time::Instant::now());

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_millis(49)).await;
        }
    });

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Happy path checkpoint flow

    // Step 1 -- get a bunch of proposals
    let mut proposals = Vec::new();
    for (auth, client) in &aggregator.authority_clients {
        let response = client
            .handle_checkpoint(CheckpointRequest::latest(true))
            .await
            .expect("No issues");

        assert!(matches!(
            response.info,
            AuthorityCheckpointInfo::Proposal { .. }
        ));

        if let AuthorityCheckpointInfo::Proposal { current, .. } = &response.info {
            assert!(current.is_some());

            proposals.push((
                *auth,
                CheckpointProposal::new(
                    current.as_ref().unwrap().clone(),
                    response.detail.unwrap(),
                ),
            ));
        }
    }

    // Step 2 -- make fragments using the proposals.
    let proposal_len = proposals.len();
    for (i, (auth, proposal)) in proposals.iter().enumerate() {
        let p0 = proposal.fragment_with(&proposals[(i + 1) % proposal_len].1);
        let p1 = proposal.fragment_with(&proposals[(i + 3) % proposal_len].1);

        let client = &aggregator.authority_clients[auth];
        client
            .handle_checkpoint(CheckpointRequest::set_fragment(p0))
            .await
            .expect("ok");
        client
            .handle_checkpoint(CheckpointRequest::set_fragment(p1))
            .await
            .expect("ok");
    }

    // Give time to the receiving task to process
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Note that some will be having a signed checkpoint and some will node
    // because they were not included in the first two links that make a checkpoint.

    // Step 3 - get the signed checkpoint
    let mut signed_checkpoint = Vec::new();
    let mut contents = None;
    let mut failed_authorities = HashSet::new();
    for (auth, client) in &aggregator.authority_clients {
        let response = client
            .handle_checkpoint(CheckpointRequest::past(0, true))
            .await
            .expect("No issues");

        match &response.info {
            AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Signed(checkpoint)) => {
                signed_checkpoint.push(checkpoint.clone());
                contents = response.detail.clone();
            }
            _ => {
                failed_authorities.insert(*auth);
            }
        }
    }

    assert!(!contents.as_ref().unwrap().transactions.is_empty());

    // Construct a certificate
    // We need at least f+1 signatures
    assert!(signed_checkpoint.len() > 1);
    let checkpoint_cert =
        CertifiedCheckpoint::aggregate(signed_checkpoint, &committee.clone()).expect("all ok");

    // Step 4 -- Upload the certificate back up.
    for (auth, client) in &aggregator.authority_clients {
        let request = if failed_authorities.contains(auth) {
            CheckpointRequest::set_checkpoint(checkpoint_cert.clone(), contents.clone())
        } else {
            // These validators already have the checkpoint
            CheckpointRequest::set_checkpoint(checkpoint_cert.clone(), None)
        };

        let response = client.handle_checkpoint(request).await.expect("No issues");
        assert!(matches!(response.info, AuthorityCheckpointInfo::Success));
    }

    // Wait for all the sending to happen.
    _end_of_sending_join.await.expect("all ok");
}
