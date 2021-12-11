// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::{
    certificate, committee, committee_with_base_port, header, headers, keys, listener, temp_dir,
    votes,
};
use futures::future::try_join_all;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn process_header() {
    let mut keys = keys();
    let _ = keys.pop().unwrap(); // Skip the header' author.
    let (name, secret) = keys.pop().unwrap();
    let mut signature_service = SignatureService::new(secret);

    let committee = committee_with_base_port(13_000);

    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create a new test store.
    let mut store = Store::new(temp_dir()).unwrap();

    // Make the vote we expect to receive.
    let expected = Vote::new(&header(), &name, &mut signature_service).await;

    // Spawn a listener to receive the vote.
    let address = committee
        .primary(&header().author)
        .unwrap()
        .primary_to_primary;
    let handle = listener(address);

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name,
        &committee,
        store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee,
        store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
    );

    // Send a header to the core.
    tx_primary_messages
        .send(PrimaryMessage::Header(header()))
        .await
        .unwrap();

    // Ensure the listener correctly received the vote.
    let received = handle.await.unwrap();
    match bincode::deserialize(&received).unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    // Ensure the header is correctly stored.
    let stored = store
        .read(header().id.to_vec())
        .await
        .unwrap()
        .map(|x| bincode::deserialize(&x).unwrap());
    assert_eq!(stored, Some(header()));
}

#[tokio::test]
async fn process_header_missing_parent() {
    let (name, secret) = keys().pop().unwrap();
    let signature_service = SignatureService::new(secret);

    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create a new test store.
    let mut store = Store::new(temp_dir()).unwrap();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name,
        &committee(),
        store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee(),
        store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
    );

    // Send a header to the core.
    let header = Header {
        parents: [Digest::default()].iter().cloned().collect(),
        ..header()
    };
    let id = header.id.clone();
    tx_primary_messages
        .send(PrimaryMessage::Header(header))
        .await
        .unwrap();

    // Ensure the header is not stored.
    assert!(store.read(id.to_vec()).await.unwrap().is_none());
}

#[tokio::test]
async fn process_header_missing_payload() {
    let (name, secret) = keys().pop().unwrap();
    let signature_service = SignatureService::new(secret);

    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create a new test store.
    let mut store = Store::new(temp_dir()).unwrap();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name,
        &committee(),
        store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee(),
        store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
    );

    // Send a header to the core.
    let header = Header {
        payload: [(Digest::default(), 0)].iter().cloned().collect(),
        ..header()
    };
    let id = header.id.clone();
    tx_primary_messages
        .send(PrimaryMessage::Header(header))
        .await
        .unwrap();

    // Ensure the header is not stored.
    assert!(store.read(id.to_vec()).await.unwrap().is_none());
}

#[tokio::test]
async fn process_votes() {
    let (name, secret) = keys().pop().unwrap();
    let signature_service = SignatureService::new(secret);

    let committee = committee_with_base_port(13_100);

    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, _rx_consensus) = channel(1);
    let (tx_parents, _rx_parents) = channel(1);

    // Create a new test store.
    let store = Store::new(temp_dir()).unwrap();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name,
        &committee,
        store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee.clone(),
        store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
    );

    // Make the certificate we expect to receive.
    let expected = certificate(&Header::default());

    // Spawn all listeners to receive our newly formed certificate.
    let handles: Vec<_> = committee
        .others_primaries(&name)
        .iter()
        .map(|(_, address)| listener(address.primary_to_primary))
        .collect();

    // Send a votes to the core.
    for vote in votes(&Header::default()) {
        tx_primary_messages
            .send(PrimaryMessage::Vote(vote))
            .await
            .unwrap();
    }

    // Ensure all listeners got the certificate.
    for received in try_join_all(handles).await.unwrap() {
        match bincode::deserialize(&received).unwrap() {
            PrimaryMessage::Certificate(x) => assert_eq!(x, expected),
            x => panic!("Unexpected message: {:?}", x),
        }
    }
}

#[tokio::test]
async fn process_certificates() {
    let (name, secret) = keys().pop().unwrap();
    let signature_service = SignatureService::new(secret);

    let (tx_sync_headers, _rx_sync_headers) = channel(1);
    let (tx_sync_certificates, _rx_sync_certificates) = channel(1);
    let (tx_primary_messages, rx_primary_messages) = channel(3);
    let (_tx_headers_loopback, rx_headers_loopback) = channel(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    let (_tx_headers, rx_headers) = channel(1);
    let (tx_consensus, mut rx_consensus) = channel(3);
    let (tx_parents, mut rx_parents) = channel(1);

    // Create a new test store.
    let mut store = Store::new(temp_dir()).unwrap();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name,
        &committee(),
        store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
    );

    // Spawn the core.
    Core::spawn(
        name,
        committee(),
        store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ Arc::new(AtomicU64::new(0)),
        /* gc_depth */ 50,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
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
    let received = rx_parents.recv().await.unwrap();
    let parents = certificates.iter().map(|x| x.digest()).collect();
    assert_eq!(received, (parents, 1));

    // Ensure the core sends the certificates to the consensus.
    for x in certificates.clone() {
        let received = rx_consensus.recv().await.unwrap();
        assert_eq!(received, x);
    }

    // Ensure the certificates are stored.
    for x in &certificates {
        let stored = store.read(x.digest().to_vec()).await.unwrap();
        let serialized = bincode::serialize(x).unwrap();
        assert_eq!(stored, Some(serialized));
    }
}
