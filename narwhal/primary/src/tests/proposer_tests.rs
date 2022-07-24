// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crypto::traits::KeyPair;
use prometheus::Registry;
use test_utils::{committee, keys};
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn propose_empty() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    let (_tx_parents, rx_parents) = channel(1);
    let (_tx_our_digests, rx_our_digests) = channel(1);
    let (tx_headers, mut rx_headers) = channel(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    Proposer::spawn(
        name,
        committee(None),
        signature_service,
        /* header_size */ 1_000,
        /* max_header_delay */ Duration::from_millis(20),
        NetworkModel::PartiallySynchronous,
        rx_reconfigure,
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        metrics,
    );

    // Ensure the proposer makes a correct empty header.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round, 1);
    assert!(header.payload.is_empty());
    assert!(header.verify(&committee(None)).is_ok());
}

#[tokio::test]
async fn propose_payload() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    let (_tx_parents, rx_parents) = channel(1);
    let (tx_our_digests, rx_our_digests) = channel(1);
    let (tx_headers, mut rx_headers) = channel(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    Proposer::spawn(
        name.clone(),
        committee(None),
        signature_service,
        /* header_size */ 32,
        /* max_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        NetworkModel::PartiallySynchronous,
        rx_reconfigure,
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        metrics,
    );

    // Send enough digests for the header payload.
    let name_bytes: [u8; 32] = *name.0.as_bytes();
    let digest = BatchDigest(name_bytes);
    let worker_id = 0;
    tx_our_digests.send((digest, worker_id)).await.unwrap();

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round, 1);
    assert_eq!(header.payload.get(&digest), Some(&worker_id));
    assert!(header.verify(&committee(None)).is_ok());
}
