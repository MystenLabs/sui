// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::create_db_stores;

use crate::primary;
use fastcrypto::traits::KeyPair;
use primary::NUM_SHUTDOWN_RECEIVERS;
use test_utils::CommitteeFixture;
use tokio::time::Duration;
use types::{
    MockPrimaryToPrimary, PreSubscribedBroadcastSender, PrimaryToPrimaryServer, RequestVoteResponse,
};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn propose_header() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, mut rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Create a fake header.
    let proposed_header = primary.header(&committee);

    // Set up network.
    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Set up remote primaries responding with votes.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.public_key() != name) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let name = primary.public_key();
        let signature_service = SignatureService::new(primary.keypair().copy());
        let vote = Vote::new(&proposed_header, &name, &signature_service).await;
        let mut mock_server = MockPrimaryToPrimary::new();
        let mut mock_seq = mockall::Sequence::new();
        // Verify errors are retried.
        mock_server
            .expect_request_vote()
            .times(3)
            .in_sequence(&mut mock_seq)
            .returning(move |_request| {
                Err(anemo::rpc::Status::new(
                    anemo::types::response::StatusCode::Unknown,
                ))
            });
        mock_server
            .expect_request_vote()
            .times(1)
            .in_sequence(&mut mock_seq)
            .return_once(move |_request| {
                Ok(anemo::Response::new(RequestVoteResponse {
                    vote: Some(vote),
                    missing: Vec::new(),
                }))
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(mock_server));
        primary_networks.push(primary.new_network(routes));
        println!("New primary added: {:?}", address);

        let address = network::multiaddr_to_address(&address).unwrap();
        let peer_id = anemo::PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // Spawn the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));
    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        rx_narwhal_round_updates,
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network,
    );

    // Propose header and ensure that a certificate is formed by pulling it out of the
    // consensus channel.
    let proposed_digest = proposed_header.digest();
    tx_headers.send(proposed_header).await.unwrap();
    let certificate = rx_consensus.recv().await.unwrap();
    assert_eq!(certificate.header.digest(), proposed_digest);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn propose_header_failure() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, mut rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Create a fake header.
    let proposed_header = primary.header(&committee);

    // Set up network.
    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Set up remote primaries responding with votes.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.public_key() != name) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let mut mock_server = MockPrimaryToPrimary::new();
        mock_server
            .expect_request_vote()
            .returning(move |_request| {
                Err(anemo::rpc::Status::new(
                    anemo::types::response::StatusCode::BadRequest, // unretriable
                ))
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(mock_server));
        primary_networks.push(primary.new_network(routes));
        println!("New primary added: {:?}", address);

        let address = network::multiaddr_to_address(&address).unwrap();
        let peer_id = anemo::PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // Spawn the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));
    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        rx_narwhal_round_updates,
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network,
    );

    // Propose header and verify we get no certificate back.
    tx_headers.send(proposed_header).await.unwrap();
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(5), rx_consensus.recv()).await {
        panic!("expected no certificate to form; got {result:?}");
    }
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, mut rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        rx_narwhal_round_updates,
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network,
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

    // TODO(metrics): Make sure that certificates_processed metric is 3
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
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
    _ = tx_shutdown.send().unwrap();

    // Restart the core.
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);

    let _core_handle = Core::spawn(
        name,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service,
        rx_consensus_round_updates,
        rx_narwhal_round_updates,
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network,
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

    // TODO(metrics): Assert that certificates_processed metric is 3
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
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
    _ = tx_shutdown.send().unwrap();

    // Restart the core.
    let (tx_certificates_restored, rx_certificates_restored) = test_utils::test_channel!(2);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates_restored,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(3);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
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
    _ = tx_shutdown.send().unwrap();

    // Restart the core.
    let (_tx_certificates_restored, rx_certificates_restored) = test_utils::test_channel!(2);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(3);
    let (tx_parents, mut rx_parents) = test_utils::test_channel!(3);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates_restored,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
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

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (_tx_certificates, rx_certificates) = test_utils::test_channel!(3);
    let (_tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        rx_consensus_round_updates.clone(),
        None,
    ));

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
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        rx_consensus_round_updates.clone(),
        rx_narwhal_round_updates.clone(),
        /* gc_depth */ 50,
        tx_shutdown.subscribe(),
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        network.clone(),
    );

    // Shutdown the core.
    _ = tx_shutdown.send().unwrap();
    assert!(handle.await.is_ok());
}
