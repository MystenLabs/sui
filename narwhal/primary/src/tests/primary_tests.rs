// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{NetworkModel, Primary, PrimaryReceiverHandler, CHANNEL_CAPACITY};
use crate::{common::create_db_stores, synchronizer::Synchronizer, NUM_SHUTDOWN_RECEIVERS};
use arc_swap::ArcSwap;
use bincode::Options;
use config::{Parameters, WorkerId};
use consensus::dag::Dag;
use crypto::PublicKey;
use fastcrypto::{
    encoding::{Encoding, Hex},
    hash::Hash,
    signature_service::SignatureService,
    traits::KeyPair,
};
use itertools::Itertools;
use std::{
    borrow::Borrow,
    collections::{BTreeSet, HashMap, HashSet},
    num::NonZeroUsize,
    sync::Arc,
    time::Duration,
};
use storage::{CertificateStore, VoteDigestStore};
use storage::{CertificateStoreCache, PayloadToken};
use storage::{NodeStorage, PayloadStore};
use store::rocks::{DBMap, MetricConf, ReadWriteOptions};
use store::Store;
use test_utils::{temp_dir, CommitteeFixture};
use tokio::sync::{mpsc, watch};

use types::{
    now, BatchDigest, Certificate, CertificateAPI, CertificateDigest, FetchCertificatesRequest,
    Header, HeaderAPI, MockPrimaryToWorker, PayloadAvailabilityRequest,
    PreSubscribedBroadcastSender, PrimaryToPrimary, RequestVoteRequest, Round,
};
use worker::{TrivialTransactionValidator, Worker};

#[tokio::test]
async fn get_network_peers_from_admin_server() {
    // telemetry_subscribers::init_for_testing();
    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let authority_1 = fixture.authorities().next().unwrap();
    let signer_1 = authority_1.keypair().copy();

    let worker_id = 0;
    let worker_1_keypair = authority_1.worker(worker_id).keypair().copy();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir(), None);
    let client_1 = NetworkClient::new_from_keypair(&authority_1.network_keypair());

    let (tx_new_certificates, rx_new_certificates) = mpsc::channel(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) = mpsc::channel(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn Primary 1
    Primary::spawn(
        authority_1.authority().clone(),
        signer_1,
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        client_1.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* dag */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    let worker_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    let mut tx_shutdown_worker = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance for primary 1.
    Worker::spawn(
        authority_1.authority().clone(),
        worker_1_keypair.copy(),
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        worker_1_parameters.clone(),
        TrivialTransactionValidator::default(),
        client_1,
        store.batch_store,
        &mut tx_shutdown_worker,
    );

    // Test getting all known peers for primary 1
    let resp = reqwest::get(format!(
        "http://127.0.0.1:{}/known_peers",
        primary_1_parameters
            .network_admin_server
            .primary_network_admin_server_port
    ))
    .await
    .unwrap()
    .json::<Vec<String>>()
    .await
    .unwrap();

    // Assert we returned 19 peers (3 other primaries + 4 workers + 4*3 other workers)
    assert_eq!(19, resp.len());

    // Test getting all connected peers for primary 1
    let resp = reqwest::get(format!(
        "http://127.0.0.1:{}/peers",
        primary_1_parameters
            .network_admin_server
            .primary_network_admin_server_port
    ))
    .await
    .unwrap()
    .json::<Vec<String>>()
    .await
    .unwrap();

    // Assert we returned 1 peers (only 1 worker spawned)
    assert_eq!(1, resp.len());

    let authority_2 = fixture.authorities().nth(1).unwrap();
    let signer_2 = authority_2.keypair().copy();
    let client_2 = NetworkClient::new_from_keypair(&authority_2.network_keypair());

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // TODO: Rework test-utils so that macro can be used for the channels below.
    let (tx_new_certificates_2, rx_new_certificates_2) = mpsc::channel(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) = mpsc::channel(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0);
    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn Primary 2
    Primary::spawn(
        authority_2.authority().clone(),
        signer_2,
        authority_2.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        client_2.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store.clone(),
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        rx_consensus_round_updates,
        /* dag */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, tx_shutdown.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown_2,
        tx_feedback_2,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    let primary_1_peer_id = Hex::encode(authority_1.network_keypair().copy().public().0.as_bytes());
    let primary_2_peer_id = Hex::encode(authority_2.network_keypair().copy().public().0.as_bytes());
    let worker_1_peer_id = Hex::encode(worker_1_keypair.copy().public().0.as_bytes());

    // Test getting all connected peers for primary 1
    let resp = reqwest::get(format!(
        "http://127.0.0.1:{}/peers",
        primary_1_parameters
            .network_admin_server
            .primary_network_admin_server_port
    ))
    .await
    .unwrap()
    .json::<Vec<String>>()
    .await
    .unwrap();

    // Assert we returned 2 peers (1 other primary spawned + 1 worker spawned)
    assert_eq!(2, resp.len());

    // Assert peer ids are correct
    let expected_peer_ids = vec![&primary_2_peer_id, &worker_1_peer_id];
    assert!(expected_peer_ids.iter().all(|e| resp.contains(e)));

    // Test getting all connected peers for primary 2
    let resp = reqwest::get(format!(
        "http://127.0.0.1:{}/peers",
        primary_2_parameters
            .network_admin_server
            .primary_network_admin_server_port
    ))
    .await
    .unwrap()
    .json::<Vec<String>>()
    .await
    .unwrap();

    // Assert we returned 2 peers (1 other primary spawned + 1 other worker)
    assert_eq!(2, resp.len());

    // Assert peer ids are correct
    let expected_peer_ids = vec![&primary_1_peer_id, &worker_1_peer_id];
    assert!(expected_peer_ids.iter().all(|e| resp.contains(e)));
}

#[tokio::test]
async fn test_request_vote_send_missing_parents() {
    telemetry_subscribers::init_for_testing();
    const NUM_PARENTS: usize = 10;
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(NUM_PARENTS).unwrap())
        .build();
    let author = fixture.authorities().next().unwrap();
    let name = author.public_key();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        target_id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: target_id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // Make some mock certificates that are parents of our new header.
    let committee: Committee = fixture.committee();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let ids: Vec<_> = fixture
        .authorities()
        .map(|a| (a.id(), a.keypair().copy()))
        .collect();
    let (certificates, _next_parents) =
        make_optimal_signed_certificates(1..=3, &genesis, &committee, ids.as_slice());
    let all_certificates = certificates.into_iter().collect_vec();
    let round_2_certs = all_certificates[NUM_PARENTS..(NUM_PARENTS * 2)].to_vec();
    let round_2_parents = round_2_certs[..(NUM_PARENTS / 2)].to_vec();
    let round_2_missing = round_2_certs[(NUM_PARENTS / 2)..].to_vec();

    // Create a test header.
    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .author(author_id)
            .round(3)
            .parents(round_2_certs.iter().map(|c| c.digest()).collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
            .build()
            .unwrap(),
    );

    // Write some certificates from round 2 into the store, and leave out the rest to test
    // headers with some parents but not all available. Round 1 certificates should be written
    // into the storage as parents of round 2 certificates. But to test phase 2 they are left out.
    for cert in round_2_parents {
        for (digest, (worker_id, _)) in cert.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
        certificate_store.write(cert.clone()).unwrap();
    }

    // TEST PHASE 1: Handler should report missing parent certificates to caller.
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());
    let result = handler.request_vote(request).await;

    let expected_missing: HashSet<_> = round_2_missing.iter().map(|c| c.digest()).collect();
    let received_missing: HashSet<_> = result.unwrap().into_body().missing.into_iter().collect();
    assert_eq!(expected_missing, received_missing);

    // TEST PHASE 2: Handler should abort if round advances too much while awaiting processing
    // of certs.
    let tx_narwhal_round_updates = Arc::new(tx_narwhal_round_updates);
    {
        let tx_narwhal_round_updates = tx_narwhal_round_updates.clone();
        tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = tx_narwhal_round_updates.send(100);
        });
    }
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: round_2_missing.clone(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());
    // Because round 1 certificates are not in store, the missing parents will not be accepted yet.
    let result = timeout(Duration::from_secs(5), handler.request_vote(request))
        .await
        .unwrap();
    assert!(result.is_err(), "{:?}", result);
    assert_eq!(
        // Returned error should be unretriable.
        anemo::types::response::StatusCode::BadRequest,
        result.err().unwrap().status()
    );

    // TODO: inject error for handling parents.
}

#[tokio::test]
async fn test_request_vote_accept_missing_parents() {
    telemetry_subscribers::init_for_testing();
    const NUM_PARENTS: usize = 10;
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(NUM_PARENTS).unwrap())
        .build();
    let target = fixture.authorities().next().unwrap();
    let author = fixture.authorities().nth(2).unwrap();
    let target_id = target.id();
    let author_id = author.id();
    let worker_cache = fixture.worker_cache();
    let signature_service = SignatureService::new(target.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let network = test_utils::test_network(target.network_keypair(), target.address());
    let client = NetworkClient::new_from_keypair(&target.network_keypair());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        target_id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: target_id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let committee: Committee = fixture.committee();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let ids: Vec<_> = fixture
        .authorities()
        .map(|a| (a.id(), a.keypair().copy()))
        .collect();
    let (certificates, _next_parents) =
        make_optimal_signed_certificates(1..=3, &genesis, &committee, ids.as_slice());
    let all_certificates = certificates.into_iter().collect_vec();
    let round_1_certs = all_certificates[..NUM_PARENTS].to_vec();
    let round_2_certs = all_certificates[NUM_PARENTS..(NUM_PARENTS * 2)].to_vec();
    let round_2_parents = round_2_certs[..(NUM_PARENTS / 2)].to_vec();
    let round_2_missing = round_2_certs[(NUM_PARENTS / 2)..].to_vec();

    // Create a test header.
    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .author(author_id)
            .round(3)
            .parents(round_2_certs.iter().map(|c| c.digest()).collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
            .build()
            .unwrap(),
    );

    // Populate all round 1 certificates and some round 2 certificates into the storage.
    // The new header will have some round 2 certificates missing as parents, but these parents
    // should be able to get accepted.
    for cert in round_1_certs {
        for (digest, (worker_id, _)) in cert.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
        certificate_store.write(cert.clone()).unwrap();
    }
    for cert in round_2_parents {
        for (digest, (worker_id, _)) in cert.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
        certificate_store.write(cert.clone()).unwrap();
    }
    // Populate new header payload so they don't have to be retrieved.
    for (digest, (worker_id, _)) in test_header.payload() {
        payload_store.write(digest, worker_id).unwrap();
    }

    // TEST PHASE 1: Handler should report missing parent certificates to caller.
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());
    let result = handler.request_vote(request).await;

    let expected_missing: HashSet<_> = round_2_missing.iter().map(|c| c.digest()).collect();
    let received_missing: HashSet<_> = result.unwrap().into_body().missing.into_iter().collect();
    assert_eq!(expected_missing, received_missing);

    // TEST PHASE 2: Handler should process missing parent certificates and succeed.
    let _ = tx_narwhal_round_updates.send(1);
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header,
        parents: round_2_missing.clone(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let result = handler.request_vote(request).await;
    assert!(result.is_ok(), "{:?}", result);
}

#[tokio::test]
async fn test_request_vote_missing_batches() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let authority_id = primary.id();
    let author = fixture.authorities().nth(2).unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        authority_id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != authority_id) {
        let header = Header::V1(
            primary
                .header_builder(&fixture.committee())
                .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());
        certificate_store.write(certificate.clone()).unwrap();
        for (digest, (worker_id, _)) in certificate.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
    }
    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .round(2)
            .parents(certificates.keys().cloned().collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1, 0)
            .build()
            .unwrap(),
    );
    let test_digests: HashSet<_> = test_header
        .payload()
        .iter()
        .map(|(digest, _)| digest)
        .cloned()
        .collect();

    // Set up mock worker.
    let author_id = author.id();
    let worker = primary.worker(1);
    let worker_address = &worker.info().worker_address;
    let worker_peer_id = anemo::PeerId(worker.keypair().public().0.to_bytes());
    let mut mock_server = MockPrimaryToWorker::new();
    mock_server
        .expect_synchronize()
        .withf(move |request| {
            let digests: HashSet<_> = request.body().digests.iter().cloned().collect();
            digests == test_digests && request.body().target == author_id
        })
        .times(1)
        .return_once(|_| Ok(anemo::Response::new(())));

    client.set_primary_to_worker_local_handler(worker_peer_id, Arc::new(mock_server));

    let _worker_network = worker.new_network(anemo::Router::new());
    let address = worker_address.to_anemo_address().unwrap();
    network
        .connect_with_peer_id(address, worker_peer_id)
        .await
        .unwrap();

    // Verify Handler synchronizes missing batches and generates a Vote.
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let response = handler.request_vote(request).await.unwrap();
    assert!(response.body().vote.is_some());
}

#[tokio::test]
async fn test_request_vote_already_voted() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let id = primary.id();
    let author = fixture.authorities().nth(2).unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));

    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let header = Header::V1(
            primary
                .header_builder(&fixture.committee())
                .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());
        certificate_store.write(certificate.clone()).unwrap();
        for (digest, (worker_id, _)) in certificate.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
    }

    // Set up mock worker.
    let worker = primary.worker(1);
    let worker_address = &worker.info().worker_address;
    let worker_peer_id = anemo::PeerId(worker.keypair().public().0.to_bytes());
    let mut mock_server = MockPrimaryToWorker::new();
    // Always Synchronize successfully.
    mock_server
        .expect_synchronize()
        .returning(|_| Ok(anemo::Response::new(())));

    client.set_primary_to_worker_local_handler(worker_peer_id, Arc::new(mock_server));

    let _worker_network = worker.new_network(anemo::Router::new());
    let address = worker_address.to_anemo_address().unwrap();
    network
        .connect_with_peer_id(address, worker_peer_id)
        .await
        .unwrap();

    // Verify Handler generates a Vote.
    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .round(2)
            .parents(certificates.keys().cloned().collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1, 0)
            .build()
            .unwrap(),
    );
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let response = handler.request_vote(request).await.unwrap();
    assert!(response.body().vote.is_some());
    let vote = response.into_body().vote.unwrap();

    // Verify the same request gets the same vote back successfully.
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let response = handler.request_vote(request).await.unwrap();
    assert!(response.body().vote.is_some());
    assert_eq!(vote.digest(), response.into_body().vote.unwrap().digest());

    // Verify a different request for the same round receives an error.
    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .round(2)
            .parents(certificates.keys().cloned().collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1, 0)
            .build()
            .unwrap(),
    );
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let response = handler.request_vote(request).await;
    assert_eq!(
        // Returned error should not be retriable.
        anemo::types::response::StatusCode::BadRequest,
        response.err().unwrap().status()
    );
}

#[tokio::test]
async fn test_fetch_certificates_handler() {
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let id = fixture.authorities().next().unwrap().id();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates.clone(),
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    let mut current_round: Vec<_> = Certificate::genesis(&fixture.committee())
        .into_iter()
        .map(|cert| cert.header().clone())
        .collect();
    let mut headers = vec![];
    let total_rounds = 4;
    for i in 0..total_rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    let total_authorities = fixture.authorities().count();
    let total_certificates = total_authorities * total_rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&header));
    }
    assert_eq!(certificates.len(), total_certificates);
    assert_eq!(16, total_certificates);

    // Populate certificate store such that each authority has the following rounds:
    // Authority 0: 1
    // Authority 1: 1 2
    // Authority 2: 1 2 3
    // Authority 3: 1 2 3 4
    // This is unrealistic because in practice a certificate can only be stored with 2f+1 parents
    // already in store. But this does not matter for testing here.
    let mut authorities = Vec::<AuthorityIdentifier>::new();
    for i in 0..total_authorities {
        authorities.push(certificates[i].header().author());
        for j in 0..=i {
            let cert = certificates[i + j * total_authorities].clone();
            assert_eq!(&cert.header().author(), authorities.last().unwrap());
            certificate_store
                .write(cert)
                .expect("Writing certificate to store failed");
        }
    }

    // Each test case contains (lower bound round, skip rounds, max items, expected output).
    let test_cases = vec![
        (
            0,
            vec![vec![], vec![], vec![], vec![]],
            20,
            vec![1, 1, 1, 1, 2, 2, 2, 3, 3, 4],
        ),
        (
            0,
            vec![vec![1u64], vec![1], vec![], vec![]],
            20,
            vec![1, 1, 2, 2, 2, 3, 3, 4],
        ),
        (
            0,
            vec![vec![], vec![], vec![1], vec![1]],
            20,
            vec![1, 1, 2, 2, 2, 3, 3, 4],
        ),
        (
            1,
            vec![vec![], vec![], vec![2], vec![2]],
            4,
            vec![2, 3, 3, 4],
        ),
        (1, vec![vec![], vec![], vec![2], vec![2]], 2, vec![2, 3]),
        (
            0,
            vec![vec![1], vec![1], vec![1, 2, 3], vec![1, 2, 3]],
            2,
            vec![2, 4],
        ),
        (2, vec![vec![], vec![], vec![], vec![]], 3, vec![3, 3, 4]),
        (2, vec![vec![], vec![], vec![], vec![]], 2, vec![3, 3]),
        // Check that round 2 and 4 are fetched for the last authority, skipping round 3.
        (
            1,
            vec![vec![], vec![], vec![3], vec![3]],
            5,
            vec![2, 2, 2, 4],
        ),
    ];
    for (lower_bound_round, skip_rounds_vec, max_items, expected_rounds) in test_cases {
        let req = FetchCertificatesRequest::default()
            .set_bounds(
                lower_bound_round,
                authorities
                    .clone()
                    .into_iter()
                    .zip(
                        skip_rounds_vec
                            .into_iter()
                            .map(|rounds| rounds.into_iter().collect()),
                    )
                    .collect(),
            )
            .set_max_items(max_items);
        let resp = handler
            .fetch_certificates(anemo::Request::new(req.clone()))
            .await
            .unwrap()
            .into_body();
        assert_eq!(
            resp.certificates
                .iter()
                .map(|cert| cert.round())
                .collect_vec(),
            expected_rounds
        );
    }
}

#[tokio::test]
async fn test_process_payload_availability_success() {
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let author = fixture.authorities().next().unwrap();
    let id = author.id();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // GIVEN some mock certificates
    let mut certificates = HashMap::new();
    let mut missing_certificates = HashSet::new();

    for i in 0..10 {
        let header = Header::V1(
            author
                .header_builder(&fixture.committee())
                .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());

        // We want to simulate the scenario of both having some certificates
        // found and some non found. Store only the half. The other half
        // should be returned back as non found.
        if i < 7 {
            // write the certificate
            certificate_store.write(certificate.clone()).unwrap();

            for (digest, (worker_id, _)) in certificate.header().payload() {
                payload_store.write(digest, worker_id).unwrap();
            }
        } else {
            missing_certificates.insert(digest);
        }
    }

    // WHEN requesting the payload availability for all the certificates
    let request = anemo::Request::new(PayloadAvailabilityRequest {
        certificate_digests: certificates.keys().copied().collect(),
    });
    let response = handler.get_payload_availability(request).await.unwrap();
    let result_digests: HashSet<CertificateDigest> = response
        .body()
        .payload_availability
        .iter()
        .map(|(digest, _)| *digest)
        .collect();

    assert_eq!(
        result_digests.len(),
        certificates.len(),
        "Returned unique number of certificates don't match the expected"
    );

    // ensure that we have no payload availability for some
    let availability_map = response
        .into_body()
        .payload_availability
        .into_iter()
        .counts_by(|c| c.1);

    for (available, found) in availability_map {
        if available {
            assert_eq!(found, 7, "Expected to have available payloads");
        } else {
            assert_eq!(found, 3, "Expected to have non available payloads");
        }
    }
}

#[tokio::test]
async fn test_process_payload_availability_when_failures() {
    // GIVEN
    // We initialise the test stores manually to allow us
    // inject some wrongly serialised values to cause data store errors.
    let rocksdb = store::rocks::open_cf(
        temp_dir(),
        None,
        MetricConf::default(),
        &[
            test_utils::CERTIFICATES_CF,
            test_utils::CERTIFICATE_DIGEST_BY_ROUND_CF,
            test_utils::CERTIFICATE_DIGEST_BY_ORIGIN_CF,
            test_utils::PAYLOAD_CF,
        ],
    )
    .expect("Failed creating database");

    let (
        certificate_map,
        certificate_digest_by_round_map,
        certificate_digest_by_origin_map,
        payload_map,
    ) = store::reopen!(&rocksdb,
        test_utils::CERTIFICATES_CF;<CertificateDigest, Certificate>,
        test_utils::CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, AuthorityIdentifier), CertificateDigest>,
        test_utils::CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(AuthorityIdentifier, Round), CertificateDigest>,
        test_utils::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    let certificate_store = CertificateStore::new(
        certificate_map,
        certificate_digest_by_round_map,
        certificate_digest_by_origin_map,
        CertificateStoreCache::new(NonZeroUsize::new(100).unwrap(), None),
    );
    let payload_store = PayloadStore::new(payload_map);

    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();
    let id = author.id();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());

    let (header_store, _, _) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // AND some mock certificates
    let mut certificate_digests = Vec::new();
    for _ in 0..10 {
        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        // In order to test an error scenario that is coming from the data store,
        // we are going to store for the provided certificate digests some unexpected
        // payload in order to blow up the deserialisation.
        let serialised_key = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .serialize(&digest.borrow())
            .expect("Couldn't serialise key");

        // Just serialise the "false" value
        let dummy_value = bcs::to_bytes(false.borrow()).expect("Couldn't serialise value");

        rocksdb
            .put_cf(
                &rocksdb
                    .cf_handle(test_utils::CERTIFICATES_CF)
                    .expect("Couldn't find column family"),
                serialised_key,
                dummy_value,
                &ReadWriteOptions::default().writeopts(),
            )
            .expect("Couldn't insert value");

        certificate_digests.push(digest);
    }

    // WHEN requesting the payload availability for all the certificates
    let request = anemo::Request::new(PayloadAvailabilityRequest {
        certificate_digests,
    });
    let result = handler.get_payload_availability(request).await;
    assert!(result.is_err(), "expected error reading certificates");
}

#[tokio::test]
async fn test_request_vote_created_at_in_future() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let id = primary.id();
    let author = fixture.authorities().nth(2).unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (_tx_synchronizer_network, rx_synchronizer_network) = oneshot::channel();

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        rx_synchronizer_network,
        None,
        metrics.clone(),
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        header_store: header_store.clone(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let header = Header::V1(
            primary
                .header_builder(&fixture.committee())
                .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());
        certificate_store.write(certificate.clone()).unwrap();
        for (digest, (worker_id, _)) in certificate.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
    }

    // Set up mock worker.
    let worker = primary.worker(1);
    let worker_address = &worker.info().worker_address;
    let worker_peer_id = anemo::PeerId(worker.keypair().public().0.to_bytes());
    let mut mock_server = MockPrimaryToWorker::new();
    // Always Synchronize successfully.
    mock_server
        .expect_synchronize()
        .returning(|_| Ok(anemo::Response::new(())));

    client.set_primary_to_worker_local_handler(worker_peer_id, Arc::new(mock_server));

    let _worker_network = worker.new_network(anemo::Router::new());
    let address = worker_address.to_anemo_address().unwrap();
    network
        .connect_with_peer_id(address, worker_peer_id)
        .await
        .unwrap();

    // Verify Handler generates a Vote.

    // Set the creation time to be deep in the future (an hour)
    let created_at = now() + 60 * 60 * 1000;

    let test_header = Header::V1(
        author
            .header_builder(&fixture.committee())
            .round(2)
            .parents(certificates.keys().cloned().collect())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1, 0)
            .created_at(created_at)
            .build()
            .unwrap(),
    );

    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    // For such a future header we get back an error
    assert!(handler.request_vote(request).await.is_err());

    // Verify Handler generates a Vote.

    // Set the creation time to be a bit in the future (500 ms)
    let created_at = now() + 500;

    let test_header = author
        .header_builder(&fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1, 0)
        .created_at(created_at)
        .build()
        .unwrap();

    let mut request = anemo::Request::new(RequestVoteRequest {
        header: Header::V1(test_header.clone()),
        parents: Vec::new(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let response = handler.request_vote(request).await.unwrap();
    assert!(response.body().vote.is_some());

    // We are now later
    assert!(created_at < now());
}
