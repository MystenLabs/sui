// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use fastcrypto::traits::KeyPair;
use indexmap::IndexMap;
use prometheus::Registry;
use test_utils::{fixture_payload, CommitteeFixture};

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
        ProposerStore::new_for_tests(),
        /* header_num_of_batches_threshold */ 32,
        /* max_header_num_of_batches */ 100,
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
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let max_num_of_batches = 10;

    // Spawn the proposer.
    let _proposer_handle = Proposer::spawn(
        name.clone(),
        committee.clone(),
        signature_service,
        ProposerStore::new_for_tests(),
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ max_num_of_batches,
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
    tx_our_digests.send((digest, worker_id, 0)).await.unwrap();

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round, 1);
    assert_eq!(header.payload.get(&digest), Some(&worker_id));
    assert!(header.verify(&committee, shared_worker_cache).is_ok());

    // WHEN available batches are more than the maximum ones
    let batches: IndexMap<BatchDigest, WorkerId> = fixture_payload((max_num_of_batches * 2) as u8);

    for (batch_id, worker_id) in batches {
        tx_our_digests.send((batch_id, worker_id, 0)).await.unwrap();
    }

    // AND send some parents to advance the round
    let parents: Vec<_> = fixture
        .headers()
        .iter()
        .take(4)
        .map(|h| fixture.certificate(h))
        .collect();

    let result = tx_parents.send((parents, 1, 0)).await;
    assert!(result.is_ok());

    // THEN the header should contain max_num_of_batches
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round, 2);
    assert_eq!(header.payload.len(), max_num_of_batches);
}

#[tokio::test]
async fn equivocation_protection() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let shared_worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let proposer_store = ProposerStore::new_for_tests();

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    let proposer_handle = Proposer::spawn(
        name.clone(),
        committee.clone(),
        signature_service.clone(),
        proposer_store.clone(),
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ 10,
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
    tx_our_digests.send((digest, worker_id, 0)).await.unwrap();

    // Create and send parents
    let parents: Vec<_> = fixture
        .headers()
        .iter()
        .take(3)
        .map(|h| fixture.certificate(h))
        .collect();

    let result = tx_parents.send((parents, 1, 0)).await;
    assert!(result.is_ok());

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.payload.get(&digest), Some(&worker_id));
    assert!(header.verify(&committee, shared_worker_cache).is_ok());

    // restart the proposer.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();
    assert!(proposer_handle.await.is_ok());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let _proposer_handle = Proposer::spawn(
        name.clone(),
        committee.clone(),
        signature_service,
        proposer_store,
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ 10,
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
    tx_our_digests.send((digest, worker_id, 0)).await.unwrap();

    // Create and send a superset parents, same round but different set from before
    let parents: Vec<_> = fixture
        .headers()
        .iter()
        .take(4)
        .map(|h| fixture.certificate(h))
        .collect();

    let result = tx_parents.send((parents, 1, 0)).await;
    assert!(result.is_ok());

    // Ensure the proposer makes the same header as before
    let new_header = rx_headers.recv().await.unwrap();
    if new_header.round == header.round {
        assert_eq!(header, new_header);
    }
}
