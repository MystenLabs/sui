// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::{create_db_stores, create_test_vote_store};
use anemo::{types::PeerInfo, PeerId};
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
use test_utils::{fixture_batch_with_transactions, CommitteeFixture, PrimaryToPrimaryMockServer};
use tokio::time::Duration;
use types::{CertificateDigest, Header, Vote};

#[tokio::test]
async fn process_header() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();

    let header = author.header(&committee);

    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let mut signature_service = SignatureService::new(primary.keypair().copy());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make the vote we expect to receive.
    let expected = Vote::new(&header, &name, &mut signature_service).await;

    // Spawn a listener to receive the vote.
    let address = committee.primary(&header.author).unwrap();
    let (mut handle, _network) =
        PrimaryToPrimaryMockServer::spawn(author.network_keypair().copy(), address.clone());

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    let address = network::multiaddr_to_address(&address).unwrap();
    let network_key = author.network_keypair().public().0.to_bytes();
    let peer_info = PeerInfo {
        peer_id: PeerId(network_key),
        affinity: anemo::types::PeerAffinity::High,
        address: vec![address],
    };
    network.known_peers().insert(peer_info);

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Send a header to the core.
    tx_primary_messages
        .send(PrimaryMessage::Header(header.clone()))
        .await
        .unwrap();

    // Ensure the listener correctly received the vote.
    match handle.recv().await.unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    // Ensure the header is correctly stored.
    let stored = header_store.read(header.id).await.unwrap();
    assert_eq!(stored, Some(header.clone()));

    let mut m = HashMap::new();
    m.insert("epoch", "0");
    m.insert("source", "other");
    assert_eq!(
        metrics.headers_processed.get_metric_with(&m).unwrap().get(),
        1
    );

    // Test idempotence by re-sending the same header and expecting the vote

    // Send the header to the core again.
    tx_primary_messages
        .send(PrimaryMessage::Header(header.clone()))
        .await
        .unwrap();

    // Ensure the listener correctly received the vote again.
    match handle.recv().await.unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    assert_eq!(
        metrics.headers_processed.get_metric_with(&m).unwrap().get(),
        2
    );
}

#[tokio::test]
async fn process_header_missing_parent() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Send a header to the core.
    let builder = types::HeaderBuilder::default();
    let header = builder
        .author(name.clone())
        .round(1)
        .epoch(0)
        .parents([CertificateDigest::default()].iter().cloned().collect())
        .with_payload_batch(fixture_batch_with_transactions(10), 0)
        .build(primary.keypair())
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
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Send a header that another node has created to the core.
    // We need this header to be another's node, because our own
    // created headers are not checked against having a payload.
    // Just take another keys other than this node's.
    let author = fixture.authorities().nth(1).unwrap();
    let header = author
        .header_builder(&committee)
        .with_payload_batch(fixture_batch_with_transactions(10), 0)
        .build(author.keypair())
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
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // TODO: duplicated code (3 times in our repo).
    for (_pubkey, addresses, network_pubkey) in committee.others_primaries(&name) {
        let peer_id = PeerId(network_pubkey.0.to_bytes());
        let address = network::multiaddr_to_address(&addresses).unwrap();
        let peer_info = PeerInfo {
            peer_id,
            affinity: anemo::types::PeerAffinity::High,
            address: vec![address],
        };
        network.known_peers().insert(peer_info);
    }

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Make the certificate we expect to receive.
    let header = Header::default();
    let expected = fixture.certificate(&header);

    // Spawn all listeners to receive our newly formed certificate.
    let mut handles: Vec<_> = fixture
        .authorities()
        .skip(1)
        .map(|a| {
            let address = committee.primary(&a.public_key()).unwrap();
            PrimaryToPrimaryMockServer::spawn(a.network_keypair().copy(), address)
        })
        .collect();

    // Send a votes to the core.
    for vote in fixture.votes(&header) {
        tx_primary_messages
            .send(PrimaryMessage::Vote(vote))
            .await
            .unwrap();
    }

    // Ensure all listeners got the certificate.
    for (handle, _network) in handles.iter_mut() {
        match handle.recv().await.unwrap() {
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
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(3);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, mut rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    // Spawn the core.
    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Send enough certificates to the core.
    let certificates: Vec<_> = fixture
        .headers()
        .iter()
        .take(3)
        .map(|h| fixture.certificate(h))
        .collect();

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
        let stored = certificates_store.read(x.digest()).unwrap();
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
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn recover_core() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(3);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers.clone(),
        tx_certificate_waiter.clone(),
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Send 2f+1 certificates to the core.
    let certificates: Vec<_> = fixture
        .headers()
        .iter()
        .take(3)
        .map(|h| fixture.certificate(h))
        .collect();

    for x in certificates.clone() {
        tx_primary_messages
            .send(PrimaryMessage::Certificate(x))
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();

    // Restart the core.
    // Make a synchronizer for the core.
    let new_synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers.clone(),
        tx_certificate_waiter.clone(),
        None,
    );
    let (_tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(3);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);

    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        new_synchronizer,
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Ensure the core sends the parents of the certificates to the proposer.

    // the recovery flow sends message that contains the parents
    let received = rx_parents.recv().await.unwrap();
    assert_eq!(received.1, 1);
    assert_eq!(received.2, 0);
    assert_eq!(received.0.len(), certificates.len());
    for c in &certificates {
        assert!(received.0.contains(c));
    }

    // Ensure the certificates are stored.
    for x in &certificates {
        let stored = certificates_store.read(x.digest()).unwrap();
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

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn recover_core_partial_certs() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(3);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers.clone(),
        tx_certificate_waiter.clone(),
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Send one certificate to the core.
    let certificates: Vec<Certificate> = fixture
        .headers()
        .iter()
        .take(3)
        .map(|h| fixture.certificate(h))
        .collect();

    let last_cert = certificates.clone().into_iter().last().unwrap();

    tx_primary_messages
        .send(PrimaryMessage::Certificate(last_cert))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();

    // Restart the core.
    // Make a synchronizer for the core.
    let new_synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );
    let (tx_primary_messages_restored, rx_primary_messages_restored) = test_utils::test_channel!(2);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        new_synchronizer,
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        /* rx_primaries */ rx_primary_messages_restored,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Send remaining 2f certs to the core.
    for x in certificates.clone().into_iter().take(2) {
        tx_primary_messages_restored
            .send(PrimaryMessage::Certificate(x))
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    for _i in 0..2 {
        let received = rx_parents.recv().await.unwrap();
        assert_eq!(received, (vec![], 0, 0));
    }

    // the recovery flow sends message that contains the parents
    let received = rx_parents.recv().await.unwrap();
    println!("{:?}", received);
    assert_eq!(received.1, 1);
    assert_eq!(received.2, 0);
    assert_eq!(received.0.len(), certificates.len());
    for c in &certificates {
        assert!(received.0.contains(c));
    }
}

#[tokio::test]
async fn shutdown_core() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Spawn the core.
    let handle = Core::spawn(
        name,
        committee.clone(),
        worker_cache,
        header_store,
        certificates_store,
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        Arc::new(PrimaryMetrics::new(&Registry::new())),
        P2pNetwork::new(network),
    );

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn reconfigure_core() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let mut signature_service = SignatureService::new(primary.keypair().copy());

    // Make the new committee & worker cache
    let mut new_committee = committee.clone();
    new_committee.epoch = 1;

    // All the channels to interface with the core.
    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_sync_headers, _rx_sync_headers) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    let (_tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make the vote we expect to receive.
    let header = author.header(&new_committee);
    let expected = Vote::new(&header, &name, &mut signature_service).await;

    // Spawn a listener to receive the vote.
    let address = new_committee.primary(&header.author).unwrap();
    let (mut handle, _network) =
        PrimaryToPrimaryMockServer::spawn(author.network_keypair().copy(), address.clone());

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store,
        /* tx_header_waiter */ tx_sync_headers,
        tx_certificate_waiter,
        None,
    );

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    let address = network::multiaddr_to_address(&address).unwrap();
    let network_key = author.network_keypair();
    let peer_info = PeerInfo {
        peer_id: PeerId(network_key.public().0.to_bytes()),
        affinity: anemo::types::PeerAffinity::High,
        address: vec![address],
    };
    network.known_peers().insert(peer_info);

    // Spawn the core.
    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        Arc::new(PrimaryMetrics::new(&Registry::new())),
        P2pNetwork::new(network),
    );

    // Change committee
    let message = ReconfigureNotification::NewEpoch(new_committee.clone());
    tx_reconfigure.send(message).unwrap();

    // Send a header to the core.
    let message = PrimaryMessage::Header(header.clone());
    tx_primary_messages.send(message).await.unwrap();

    // Ensure the listener correctly received the vote.
    match handle.recv().await.unwrap() {
        PrimaryMessage::Vote(x) => assert_eq!(x, expected),
        x => panic!("Unexpected message: {:?}", x),
    }

    // Ensure the header is correctly stored.
    let stored = header_store.read(header.id).await.unwrap();
    assert_eq!(stored, Some(header));
}
