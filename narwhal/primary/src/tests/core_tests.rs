// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::create_db_stores;

use fastcrypto::traits::KeyPair;
use prometheus::Registry;
use test_utils::CommitteeFixture;
use tokio::time::Duration;

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
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, mut rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        certificate_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure,
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
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
        tx_certificates.send((x, None)).await.unwrap();
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
        let stored = certificate_store.read(x.digest()).unwrap();
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
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
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
        tx_certificates.send((x, None)).await.unwrap();
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();

    // Restart the core.
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);

    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
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
        let stored = certificate_store.read(x.digest()).unwrap();
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
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
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

    tx_certificates.send((last_cert, None)).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();

    // Restart the core.
    let (tx_certificates_restored, rx_certificates_restored) = test_utils::test_channel!(2);
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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates_restored,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Send remaining 2f certs to the core.
    for x in certificates.clone().into_iter().take(2) {
        tx_certificates_restored.send((x, None)).await.unwrap();
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

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn recover_core_expecting_header_of_previous_round() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Send 2f+1 certificates for round r, and 1 cert of round r + 1 to the core.
    let certificates: Vec<Certificate> = fixture
        .headers()
        .iter()
        .take(3)
        .map(|h| fixture.certificate(h))
        .collect();

    let certificates_next_round: Vec<Certificate> = fixture
        .headers_next_round()
        .iter()
        .take(1)
        .map(|h| fixture.certificate(h))
        .collect();

    for x in certificates.clone() {
        tx_certificates.send((x, None)).await.unwrap();
    }

    for x in certificates_next_round.clone() {
        tx_certificates.send((x, None)).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown the core.
    let shutdown = ReconfigureNotification::Shutdown;
    tx_reconfigure.send(shutdown).unwrap();

    // Restart the core.
    let (_tx_certificates_restored, rx_certificates_restored) = test_utils::test_channel!(2);
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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates_restored,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // the recovery flow sends message that contains the parents for the last round for which we
    // have a quorum of certificates, in this case is round 1.
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
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Spawn the core.
    let handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
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
    let primary = fixture.authorities().nth(1).unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    // Make the new committee & worker cache
    let mut new_committee = committee.clone();
    new_committee.epoch = 1;

    // All the channels to interface with the core.
    let (tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        /* gc_depth */ 50,
        rx_reconfigure.clone(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Change committee
    let message = ReconfigureNotification::NewEpoch(new_committee.clone());
    tx_reconfigure.send(message).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
}
