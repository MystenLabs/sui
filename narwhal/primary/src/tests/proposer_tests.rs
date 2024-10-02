// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::consensus::LeaderSwapTable;
use crate::NUM_SHUTDOWN_RECEIVERS;
use indexmap::IndexMap;
use prometheus::Registry;
use test_utils::{fixture_payload, latest_protocol_version, CommitteeFixture};
use types::PreSubscribedBroadcastSender;

#[tokio::test]
async fn propose_empty() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.id();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (_tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (_tx_committed_own_headers, rx_committed_own_headers) = test_utils::test_channel!(1);
    let (_tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);
    let (tx_narwhal_round_updates, _rx_narwhal_round_updates) = watch::channel(0u64);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    let _proposer_handle = Proposer::spawn(
        name,
        committee.clone(),
        &latest_protocol_version(),
        ProposerStore::new_for_tests(),
        /* header_num_of_batches_threshold */ 32,
        /* max_header_num_of_batches */ 100,
        /* max_header_delay */ Duration::from_millis(20),
        /* min_header_delay */ Duration::from_millis(20),
        None,
        tx_shutdown.subscribe(),
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        tx_narwhal_round_updates,
        rx_committed_own_headers,
        metrics,
        LeaderSchedule::new(committee.clone(), LeaderSwapTable::default()),
    );

    // Ensure the proposer makes a correct empty header.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round(), 1);
    assert!(header.payload().is_empty());
    assert!(header.validate(&committee, &worker_cache).is_ok());
}

#[tokio::test]
async fn propose_payload_and_repropose_after_n_seconds() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.id();
    let header_resend_delay = Duration::from_secs(3);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (_tx_committed_own_headers, rx_committed_own_headers) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);
    let (tx_narwhal_round_updates, _rx_narwhal_round_updates) = watch::channel(0u64);

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let max_num_of_batches = 10;

    // Spawn the proposer.
    let _proposer_handle = Proposer::spawn(
        name,
        committee.clone(),
        &latest_protocol_version(),
        ProposerStore::new_for_tests(),
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ max_num_of_batches,
        /* max_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* min_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        Some(header_resend_delay),
        tx_shutdown.subscribe(),
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        tx_narwhal_round_updates,
        rx_committed_own_headers,
        metrics,
        LeaderSchedule::new(committee.clone(), LeaderSwapTable::default()),
    );

    // Send enough digests for the header payload.
    let mut b = [0u8; 32];
    let r: Vec<u8> = (0..32).map(|_v| rand::random::<u8>()).collect();
    b.copy_from_slice(r.as_slice());

    let digest = BatchDigest(b);
    let worker_id = 0;
    let created_at_ts = 0;
    let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();
    tx_our_digests
        .send(OurDigestMessage {
            digest,
            worker_id,
            timestamp: created_at_ts,
            ack_channel: Some(tx_ack),
        })
        .await
        .unwrap();

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round(), 1);
    assert_eq!(
        header.payload().get(&digest),
        Some(&(worker_id, created_at_ts))
    );
    assert!(header.validate(&committee, &worker_cache).is_ok());

    // WHEN available batches are more than the maximum ones
    let batches: IndexMap<BatchDigest, (WorkerId, TimestampMs)> =
        fixture_payload((max_num_of_batches * 2) as u8, &latest_protocol_version());

    let mut ack_list = vec![];
    for (batch_id, (worker_id, created_at)) in batches {
        let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();
        tx_our_digests
            .send(OurDigestMessage {
                digest: batch_id,
                worker_id,
                timestamp: created_at,
                ack_channel: Some(tx_ack),
            })
            .await
            .unwrap();

        ack_list.push(rx_ack);

        tokio::task::yield_now().await;
    }

    // AND send some parents to advance the round
    let parents: Vec<_> = fixture
        .headers(&latest_protocol_version())
        .iter()
        .take(4)
        .map(|h| fixture.certificate(&latest_protocol_version(), h))
        .collect();

    let result = tx_parents.send((parents, 1)).await;
    assert!(result.is_ok());

    // THEN the header should contain max_num_of_batches
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(header.round(), 2);
    assert_eq!(header.payload().len(), max_num_of_batches);
    assert!(rx_ack.await.is_ok());

    // Check all batches are acked.
    for rx_ack in ack_list {
        assert!(rx_ack.await.is_ok());
    }

    // WHEN wait to fetch again from the rx_headers a few times.
    // In theory after header_resend_delay we should receive again
    // the last created header.
    for _ in 0..3 {
        let resent_header = rx_headers.recv().await.unwrap();

        // THEN should be the exact same as the last sent
        assert_eq!(header, resent_header);
    }
}

#[tokio::test]
async fn equivocation_protection() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let authority_id = primary.id();
    let proposer_store = ProposerStore::new_for_tests();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);
    let (tx_narwhal_round_updates, _rx_narwhal_round_updates) = watch::channel(0u64);
    let (_tx_committed_own_headers, rx_committed_own_headers) = test_utils::test_channel!(1);
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the proposer.
    let proposer_handle = Proposer::spawn(
        authority_id,
        committee.clone(),
        &latest_protocol_version(),
        proposer_store.clone(),
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ 10,
        /* max_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* min_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        None,
        tx_shutdown.subscribe(),
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        tx_narwhal_round_updates,
        rx_committed_own_headers,
        metrics,
        LeaderSchedule::new(committee.clone(), LeaderSwapTable::default()),
    );

    // Send enough digests for the header payload.
    let mut b = [0u8; 32];
    let r: Vec<u8> = (0..32).map(|_v| rand::random::<u8>()).collect();
    b.copy_from_slice(r.as_slice());

    let digest = BatchDigest(b);
    let worker_id = 0;
    let created_at_ts = 0;
    let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();
    tx_our_digests
        .send(OurDigestMessage {
            digest,
            worker_id,
            timestamp: created_at_ts,
            ack_channel: Some(tx_ack),
        })
        .await
        .unwrap();

    // Create and send parents
    let parents: Vec<_> = fixture
        .headers(&latest_protocol_version())
        .iter()
        .take(3)
        .map(|h| fixture.certificate(&latest_protocol_version(), h))
        .collect();

    let result = tx_parents.send((parents, 1)).await;
    assert!(result.is_ok());
    assert!(rx_ack.await.is_ok());

    // Ensure the proposer makes a correct header from the provided payload.
    let header = rx_headers.recv().await.unwrap();
    assert_eq!(
        header.payload().get(&digest),
        Some(&(worker_id, created_at_ts))
    );
    assert!(header.validate(&committee, &worker_cache).is_ok());

    // restart the proposer.
    tx_shutdown.send().unwrap();
    assert!(proposer_handle.await.is_ok());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_parents, rx_parents) = test_utils::test_channel!(1);
    let (tx_our_digests, rx_our_digests) = test_utils::test_channel!(1);
    let (tx_headers, mut rx_headers) = test_utils::test_channel!(1);
    let (tx_narwhal_round_updates, _rx_narwhal_round_updates) = watch::channel(0u64);
    let (_tx_committed_own_headers, rx_committed_own_headers) = test_utils::test_channel!(1);
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let _proposer_handle = Proposer::spawn(
        authority_id,
        committee.clone(),
        &latest_protocol_version(),
        proposer_store,
        /* header_num_of_batches_threshold */ 1,
        /* max_header_num_of_batches */ 10,
        /* max_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* min_header_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        None,
        tx_shutdown.subscribe(),
        /* rx_core */ rx_parents,
        /* rx_workers */ rx_our_digests,
        /* tx_core */ tx_headers,
        tx_narwhal_round_updates,
        rx_committed_own_headers,
        metrics,
        LeaderSchedule::new(committee.clone(), LeaderSwapTable::default()),
    );

    // Send enough digests for the header payload.
    let mut b = [0u8; 32];
    let r: Vec<u8> = (0..32).map(|_v| rand::random::<u8>()).collect();
    b.copy_from_slice(r.as_slice());

    let digest = BatchDigest(b);
    let worker_id = 0;
    let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();
    tx_our_digests
        .send(OurDigestMessage {
            digest,
            worker_id,
            timestamp: 0,
            ack_channel: Some(tx_ack),
        })
        .await
        .unwrap();

    // Create and send a superset parents, same round but different set from before
    let parents: Vec<_> = fixture
        .headers(&latest_protocol_version())
        .iter()
        .take(4)
        .map(|h| fixture.certificate(&latest_protocol_version(), h))
        .collect();

    let result = tx_parents.send((parents, 1)).await;
    assert!(result.is_ok());
    assert!(rx_ack.await.is_ok());

    // Ensure the proposer makes the same header as before
    let new_header = rx_headers.recv().await.unwrap();
    if new_header.round() == header.round() {
        assert_eq!(header, new_header);
    }
}
