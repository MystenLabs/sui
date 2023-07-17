// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::create_db_stores;

use crate::{primary, PrimaryChannelMetrics};
use consensus::consensus::ConsensusRound;
use crypto::KeyPair as DefinedKeyPair;
use fastcrypto::traits::KeyPair;
use network::client::NetworkClient;
use primary::NUM_SHUTDOWN_RECEIVERS;
use prometheus::Registry;
use rand::{rngs::StdRng, SeedableRng};
use std::num::NonZeroUsize;
use test_utils::CommitteeFixture;
use tokio::sync::watch;
use tokio::time::Duration;
use types::{
    CertificateAPI, MockPrimaryToPrimary, PreSubscribedBroadcastSender, PrimaryToPrimaryServer,
    RequestVoteResponse,
};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn propose_header() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let id = primary.id();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_new_certificates, mut rx_new_certificates) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Create a fake header.
    let proposed_header = primary.header(&committee);

    // Set up network.
    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Set up remote primaries responding with votes.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let name = primary.id();
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

        let address = address.to_anemo_address().unwrap();
        let peer_id = anemo::PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // Spawn the core.
    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        rx_synchronizer_network,
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let _handle = Certifier::spawn(
        id,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        tx_shutdown.subscribe(),
        rx_headers,
        metrics.clone(),
        network,
    );

    // Propose header and ensure that a certificate is formed by pulling it out of the
    // consensus channel.
    let proposed_digest = proposed_header.digest();
    tx_headers.send(proposed_header).await.unwrap();
    let certificate = rx_new_certificates.recv().await.unwrap();
    assert_eq!(certificate.header().digest(), proposed_digest);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn propose_header_failure() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let authority_id = primary.id();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_new_certificates, mut rx_new_certificates) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Create a fake header.
    let proposed_header = primary.header(&committee);

    // Set up network.
    let own_address = committee
        .primary_by_id(&authority_id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Set up remote primaries responding with votes.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.id() != authority_id) {
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

        let address = address.to_anemo_address().unwrap();
        let peer_id = anemo::PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // Spawn the core.
    let synchronizer = Arc::new(Synchronizer::new(
        authority_id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        rx_synchronizer_network,
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let _handle = Certifier::spawn(
        authority_id,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        tx_shutdown.subscribe(),
        rx_headers,
        metrics.clone(),
        network,
    );

    // Propose header and verify we get no certificate back.
    tx_headers.send(proposed_header).await.unwrap();
    if let Ok(result) =
        tokio::time::timeout(Duration::from_secs(5), rx_new_certificates.recv()).await
    {
        panic!("expected no certificate to form; got {result:?}");
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn propose_header_scenario_with_bad_sigs() {
    telemetry_subscribers::init_for_testing();
    // expect cert if less than 2 byzantines, otherwise no cert
    run_vote_aggregator_with_param(6, 0, true).await;
    run_vote_aggregator_with_param(6, 1, true).await;
    run_vote_aggregator_with_param(6, 2, false).await;

    // expect cert if less than 2 byzantines, otherwise no cert
    run_vote_aggregator_with_param(4, 0, true).await;
    run_vote_aggregator_with_param(4, 1, true).await;
    run_vote_aggregator_with_param(4, 2, false).await;
}

async fn run_vote_aggregator_with_param(
    committee_size: usize,
    num_byzantine: usize,
    expect_cert: bool,
) {
    let fixture = CommitteeFixture::builder()
        .committee_size(NonZeroUsize::new(committee_size).unwrap())
        .randomize_ports(true)
        .build();

    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().last().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let id: AuthorityIdentifier = primary.id();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_new_certificates, mut rx_new_certificates) = test_utils::test_channel!(3);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Create a fake header.
    let proposed_header = primary.header(&committee);

    // Set up network.
    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Set up remote primaries responding with votes.
    let mut primary_networks = Vec::new();
    for (i, primary) in fixture.authorities().filter(|a| a.id() != id).enumerate() {
        let address = committee.primary(&primary.public_key()).unwrap();
        let name = primary.id();
        // Create bad signature for a number of byzantines.
        let vote = if i < num_byzantine {
            let bad_key: DefinedKeyPair = DefinedKeyPair::generate(&mut StdRng::from_seed([0; 32]));
            Vote::new_with_signer(&proposed_header, &name, &bad_key)
        } else {
            Vote::new_with_signer(&proposed_header, &name, primary.keypair())
        };
        let mut mock_server = MockPrimaryToPrimary::new();
        let mut mock_seq = mockall::Sequence::new();
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

        let address = address.to_anemo_address().unwrap();
        let peer_id = anemo::PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // Spawn the core.
    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        rx_synchronizer_network,
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let _handle = Certifier::spawn(
        id,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer,
        signature_service,
        tx_shutdown.subscribe(),
        rx_headers,
        metrics.clone(),
        network,
    );

    // Send a proposed header.
    let proposed_digest = proposed_header.digest();
    tx_headers.send(proposed_header).await.unwrap();

    if expect_cert {
        // A cert is expected, checks that the header digest matches.
        let certificate = rx_new_certificates.recv().await.unwrap();
        assert_eq!(certificate.header().digest(), proposed_digest);
    } else {
        // A cert is not expected, checks that it times out without forming the cert.
        assert!(
            tokio::time::timeout(Duration::from_secs(5), rx_new_certificates.recv())
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn shutdown_core() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let id: AuthorityIdentifier = primary.id();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    // Create test stores.
    let (header_store, certificate_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        rx_synchronizer_network,
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // Spawn the core.
    let handle = Certifier::spawn(
        id,
        committee.clone(),
        header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service.clone(),
        tx_shutdown.subscribe(),
        rx_headers,
        metrics.clone(),
        network.clone(),
    );

    // Shutdown the core.
    _ = tx_shutdown.send().unwrap();
    assert!(handle.await.is_ok());
}
