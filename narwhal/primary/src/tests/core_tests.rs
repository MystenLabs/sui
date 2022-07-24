// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::create_db_stores;
use crypto::{ed25519::Ed25519PublicKey, traits::KeyPair};
use prometheus::Registry;
use test_utils::{
    certificate, committee, fixture_batch_with_transactions, header, headers, keys, votes,
    PrimaryToPrimaryMockServer,
};
use tokio::sync::mpsc::channel;
use types::{Header, Vote};

#[tokio::test]
async fn process_header() {
    let mut keys = keys(None);
    let _ = keys.pop().unwrap(); // Skip the header' author.
    let kp = keys.pop().unwrap();
    let name = kp.public().clone();
    let mut signature_service = SignatureService::new(kp);

    let committee = committee(None);

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make the vote we expect to receive.
    let expected = Vote::new(&header(), &name, &mut signature_service).await;

    // Spawn a listener to receive the vote.
    let address = committee
        .primary(&header().author)
        .unwrap()
        .primary_to_primary;
    let mut handle = PrimaryToPrimaryMockServer::spawn(address);

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the core.
    Core::spawn(
        name,
        committee.clone(),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Send a header to the core.
    tx_primary_messages
        .send(PrimaryMessage::Header(header()))
        .await
        .unwrap();

    // Ensure the listener correctly received the vote.
    let received = handle.recv().await.unwrap();
    match received.deserialize().unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    // Ensure the header is correctly stored.
    let stored = header_store.read(header().id).await.unwrap();
    assert_eq!(stored, Some(header()));

    let mut m = HashMap::new();
    m.insert("epoch", "0");
    m.insert("source", "other");
    assert_eq!(
        metrics.headers_processed.get_metric_with(&m).unwrap().get(),
        1
    );
}

#[tokio::test]
async fn process_header_missing_parent() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the core.
    Core::spawn(
        name.clone(),
        committee(None),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Send a header to the core.
    let kp = keys(None).pop().unwrap();
    let builder = types::HeaderBuilder::<Ed25519PublicKey>::default();
    let header = builder
        .author(name.clone())
        .round(1)
        .epoch(0)
        .parents([CertificateDigest::default()].iter().cloned().collect())
        .with_payload_batch(fixture_batch_with_transactions(10), 0)
        .build(&kp)
        .unwrap();

    let id = header.id;
    tx_primary_messages
        .send(PrimaryMessage::Header(header))
        .await
        .unwrap();

    // Ensure the header is not stored.
    assert!(header_store.read(id).await.unwrap().is_none());
}

#[tokio::test]
async fn process_header_missing_payload() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the core.
    Core::spawn(
        name.clone(),
        committee(None),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Send a header that another node has created to the core.
    // We need this header to be another's node, because our own
    // created headers are not checked against having a payload.
    // Just take another keys other than this node's.
    let keys = keys(None);
    let kp = keys.get(1).unwrap();
    let name = kp.public().clone();
    let builder = types::HeaderBuilder::<Ed25519PublicKey>::default();
    let header = builder
        .author(name.clone())
        .round(1)
        .epoch(0)
        .parents(
            Certificate::genesis(&committee(None))
                .iter()
                .map(|x| x.digest())
                .collect(),
        )
        .with_payload_batch(fixture_batch_with_transactions(10), 0)
        .build(kp)
        .unwrap();

    let id = header.id;
    tx_primary_messages
        .send(PrimaryMessage::Header(header))
        .await
        .unwrap();

    // Ensure the header is not stored.
    assert!(header_store.read(id).await.unwrap().is_none());
}

#[tokio::test]
async fn process_votes() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let committee = committee(None);

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the core.
    Core::spawn(
        name.clone(),
        committee.clone(),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Make the certificate we expect to receive.
    let expected = certificate(&Header::default());

    // Spawn all listeners to receive our newly formed certificate.
    let mut handles: Vec<_> = committee
        .others_primaries(&name)
        .into_iter()
        .map(|(_, address)| PrimaryToPrimaryMockServer::spawn(address.primary_to_primary))
        .collect();

    // Send a votes to the core.
    for vote in votes(&Header::default()) {
        tx_primary_messages
            .send(PrimaryMessage::Vote(vote))
            .await
            .unwrap();
    }

    // Ensure all listeners got the certificate.
    for handle in handles.iter_mut() {
        let received = handle.recv().await.unwrap();
        match received.deserialize().unwrap() {
            PrimaryMessage::Certificate(x) => assert_eq!(x, expected),
            x => panic!("Unexpected message: {:?}", x),
        }
    }

    let mut m = HashMap::new();
    m.insert("epoch", "0");
    assert_eq!(
        metrics
            .certificates_created
            .get_metric_with(&m)
            .unwrap()
            .get(),
        1
    );
}

#[tokio::test]
async fn process_certificates() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(3);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, mut rx_consensus) = channel(3);
    let (tx_parents, mut rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    // Spawn the core.
    Core::spawn(
        name,
        committee(None),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Send enough certificates to the core.
    let certificates: Vec<_> = headers().iter().take(3).map(certificate).collect();

    for x in certificates.clone() {
        tx_primary_messages
            .send(PrimaryMessage::Certificate(x))
            .await
            .unwrap();
    }

    // Ensure the core sends the parents of the certificates to the proposer.
    //
    // The first messages are the core letting us know about the round of parent certificates
    for _i in 0..3 {
        let received = rx_parents.recv().await.unwrap();
        assert_eq!(received, (vec![], 0, 0));
    }
    // the next message actually contains the parents
    let received = rx_parents.recv().await.unwrap();
    assert_eq!(received, (certificates.clone(), 1, 0));

    // Ensure the core sends the certificates to the consensus.
    for x in certificates.clone() {
        let received = rx_consensus.recv().await.unwrap();
        assert_eq!(received, x);
    }

    // Ensure the certificates are stored.
    for x in &certificates {
        let stored = certificates_store.read(x.digest()).await.unwrap();
        assert_eq!(stored, Some(x.clone()));
    }

    let mut m = HashMap::new();
    m.insert("epoch", "0");
    m.insert("source", "other");
    assert_eq!(
        metrics
            .certificates_processed
            .get_metric_with(&m)
            .unwrap()
            .get(),
        3
    );
}

#[tokio::test]
async fn shutdown_core() {
    let mut keys = keys(None);
    let _ = keys.pop().unwrap(); // Skip the header' author.
    let kp = keys.pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    let committee = committee(None);

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (_tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    // Spawn the core.
    let handle = Core::spawn(
        name,
        committee.clone(),
        header_store,
        certificates_store,
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        Arc::new(PrimaryMetrics::new(&Registry::new())),
    );

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn reconfigure_core() {
    let mut keys = keys(None);
    let _ = keys.pop().unwrap(); // Skip the header' author.
    let kp = keys.pop().unwrap();
    let name = kp.public().clone();
    let mut signature_service = SignatureService::new(kp);

    // Make the current and new committee.
    let committee = committee(None);
    let mut new_committee = test_utils::committee(None);
    new_committee.epoch = 1;

    // All the channels to interface with the core.
    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make the vote we expect to receive.
    let header = test_utils::header_with_epoch(&new_committee);
    let expected = Vote::new(&header, &name, &mut signature_service).await;

    // Spawn a listener to receive the vote.
    let address = new_committee
        .primary(&header.author)
        .unwrap()
        .primary_to_primary;
    let mut handle = PrimaryToPrimaryMockServer::spawn(address);

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee.clone(),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        Arc::new(PrimaryMetrics::new(&Registry::new())),
    );

    // Change committee
    let message = ReconfigureNotification::NewCommittee(new_committee.clone());
    tx_reconfigure.send(message).unwrap();

    // Send a header to the core.
    let message = PrimaryMessage::Header(header.clone());
    tx_primary_messages.send(message).await.unwrap();

    // Ensure the listener correctly received the vote.
    let received = handle.recv().await.unwrap();
    match received.deserialize().unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    // Ensure the header is correctly stored.
    let stored = header_store.read(header.id).await.unwrap();
    assert_eq!(stored, Some(header));
}
