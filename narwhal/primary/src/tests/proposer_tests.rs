// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
use test_utils::CommitteeFixture;

#[tokio::test]
async fn propose_empty() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let shared_worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (_tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    let _proposer_handle = Proposer::spawn(
        name,
        committee.clone(),
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
    assert!(header.verify(&committee, shared_worker_cache).is_ok());
}

#[tokio::test]
async fn propose_payload() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let shared_worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    let _proposer_handle = Proposer::spawn(
        name.clone(),
        committee.clone(),
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
    let mut name_bytes = [0u8; 32];
    name_bytes.copy_from_slice(&name.as_ref()[..32]);

    let digest = BatchDigest(name_bytes);
    let worker_id = 0;
    tx_our_digests.send((digest, worker_id)).await.unwrap();

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round, 1);
    assert_eq!(header.payload.get(&digest), Some(&worker_id));
    assert!(header.verify(&committee, shared_worker_cache).is_ok());
}
