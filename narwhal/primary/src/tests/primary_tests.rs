// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{Primary, PrimaryReceiverHandler, CHANNEL_CAPACITY};
use crate::{
    common::create_db_stores,
    consensus::{ConsensusRound, LeaderSchedule, LeaderSwapTable},
    metrics::{PrimaryChannelMetrics, PrimaryMetrics},
    synchronizer::Synchronizer,
    NUM_SHUTDOWN_RECEIVERS,
};

use config::{AuthorityIdentifier, ChainIdentifier, Committee, Parameters};
use fastcrypto::{
    encoding::{Encoding, Hex},
    hash::Hash,
    signature_service::SignatureService,
    traits::KeyPair as _,
};
use itertools::Itertools;
use network::client::NetworkClient;
use prometheus::Registry;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
    time::Duration,
};
use storage::{NodeStorage, VoteDigestStore};
use test_utils::{
    get_protocol_config, latest_protocol_version, make_optimal_signed_certificates, temp_dir,
    CommitteeFixture,
};
use tokio::{sync::watch, time::timeout};
use types::{
    now, Certificate, CertificateAPI, FetchCertificatesRequest, Header, HeaderAPI,
    MockPrimaryToWorker, PreSubscribedBroadcastSender, PrimaryToPrimary, RequestVoteRequest,
    SignatureVerificationState, VoteAPI,
};
use worker::{metrics::initialise_metrics, TrivialTransactionValidator, Worker};

#[tokio::test]
async fn test_get_network_peers_from_admin_server() {
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

    let (tx_new_certificates, _rx_new_certificates) = mysten_metrics::metered_channel::channel(
        CHANNEL_CAPACITY,
        &prometheus::IntGauge::new(
            PrimaryChannelMetrics::NAME_NEW_CERTS,
            PrimaryChannelMetrics::DESC_NEW_CERTS,
        )
        .unwrap(),
    );
    let (tx_feedback, rx_feedback) = mysten_metrics::metered_channel::channel(
        CHANNEL_CAPACITY,
        &prometheus::IntGauge::new(
            PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
            PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
        )
        .unwrap(),
    );
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn Primary 1
    Primary::spawn(
        authority_1.authority().clone(),
        signer_1,
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        ChainIdentifier::unknown(),
        test_utils::latest_protocol_version(),
        primary_1_parameters.clone(),
        client_1.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store.clone(),
        store.randomness_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        &mut tx_shutdown,
        tx_feedback,
        &Registry::new(),
        LeaderSchedule::new(committee.clone(), LeaderSwapTable::default()),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    let registry_1 = Registry::new();
    let metrics_1 = initialise_metrics(&registry_1);

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
        test_utils::latest_protocol_version(),
        worker_1_parameters.clone(),
        TrivialTransactionValidator,
        client_1,
        store.batch_store,
        metrics_1,
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
    let (tx_new_certificates_2, _rx_new_certificates_2) = mysten_metrics::metered_channel::channel(
        CHANNEL_CAPACITY,
        &prometheus::IntGauge::new(
            PrimaryChannelMetrics::NAME_NEW_CERTS,
            PrimaryChannelMetrics::DESC_NEW_CERTS,
        )
        .unwrap(),
    );
    let (tx_feedback_2, rx_feedback_2) = mysten_metrics::metered_channel::channel(
        CHANNEL_CAPACITY,
        &prometheus::IntGauge::new(
            PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
            PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
        )
        .unwrap(),
    );
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn Primary 2
    Primary::spawn(
        authority_2.authority().clone(),
        signer_2,
        authority_2.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        ChainIdentifier::unknown(),
        test_utils::latest_protocol_version(),
        primary_2_parameters.clone(),
        client_2.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store.clone(),
        store.randomness_store.clone(),
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        rx_consensus_round_updates,
        &mut tx_shutdown_2,
        tx_feedback_2,
        &Registry::new(),
        LeaderSchedule::new(committee, LeaderSwapTable::default()),
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
    let expected_peer_ids = [&primary_2_peer_id, &worker_1_peer_id];
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
    let expected_peer_ids = [&primary_1_peer_id, &worker_1_peer_id];
    assert!(expected_peer_ids.iter().all(|e| resp.contains(e)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_request_vote_has_missing_parents() {
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
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let network = test_utils::test_network(target.network_keypair(), target.address());
    let client = NetworkClient::new_from_keypair(&target.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        target_id,
        fixture.committee(),
        latest_protocol_version(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: target_id,
        committee: fixture.committee(),
        protocol_config: latest_protocol_version(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let committee: Committee = fixture.committee();
    let genesis = Certificate::genesis(&latest_protocol_version(), &committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let ids: Vec<_> = fixture
        .authorities()
        .map(|a| (a.id(), a.keypair().copy()))
        .collect();
    let (certificates, _next_parents) = make_optimal_signed_certificates(
        1..=3,
        &genesis,
        &committee,
        &test_utils::latest_protocol_version(),
        ids.as_slice(),
    );
    let all_certificates = certificates.into_iter().collect_vec();
    let round_2_certs = all_certificates[NUM_PARENTS..(NUM_PARENTS * 2)].to_vec();
    let round_2_parents = round_2_certs[..(NUM_PARENTS / 2)].to_vec();
    let round_2_missing = round_2_certs[(NUM_PARENTS / 2)..].to_vec();

    // Create a test header.
    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .author(author_id)
        .round(3)
        .parents(round_2_certs.iter().map(|c| c.digest()).collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            0,
            0,
        )
        .build()
        .unwrap()
        .into();

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

    // TEST PHASE 2: Handler should not return additional unknown digests.
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
    // No additional missing parents will be requested.
    let result = timeout(Duration::from_secs(5), handler.request_vote(request)).await;
    assert!(result.is_err(), "{:?}", result);

    // TEST PHASE 3: Handler should return error if header is too old.
    // Increase round threshold.
    let _ = tx_narwhal_round_updates.send(100);
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
    // Because round 1 certificates are not in store, the missing parents will not be accepted yet.
    let result = handler.request_vote(request).await;
    assert!(result.is_err(), "{:?}", result);
    assert_eq!(
        // Returned error should be unretriable.
        anemo::types::response::StatusCode::BadRequest,
        result.err().unwrap().status()
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
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
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let network = test_utils::test_network(target.network_keypair(), target.address());
    let client = NetworkClient::new_from_keypair(&target.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        target_id,
        fixture.committee(),
        latest_protocol_version(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: target_id,
        committee: fixture.committee(),
        protocol_config: latest_protocol_version(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let committee: Committee = fixture.committee();
    let genesis = Certificate::genesis(&latest_protocol_version(), &committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let ids: Vec<_> = fixture
        .authorities()
        .map(|a| (a.id(), a.keypair().copy()))
        .collect();
    let (certificates, _next_parents) = make_optimal_signed_certificates(
        1..=3,
        &genesis,
        &committee,
        &test_utils::latest_protocol_version(),
        ids.as_slice(),
    );
    let all_certificates = certificates.into_iter().collect_vec();
    let round_1_certs = all_certificates[..NUM_PARENTS].to_vec();
    let round_2_certs = all_certificates[NUM_PARENTS..(NUM_PARENTS * 2)].to_vec();
    let round_2_parents = round_2_certs[..(NUM_PARENTS / 2)].to_vec();
    let round_2_missing = round_2_certs[(NUM_PARENTS / 2)..].to_vec();

    // Create a test header.
    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .author(author_id)
        .round(3)
        .parents(round_2_certs.iter().map(|c| c.digest()).collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            0,
            0,
        )
        .build()
        .unwrap()
        .into();

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

    // TEST PHASE 1.5: Send parents with the incorrect version.
    let mut cert_v1_round_2_missing = vec![];
    let cert_v1_config = get_protocol_config(28);
    for cert in round_2_missing.iter() {
        let mut signatures = Vec::new();
        for authority in fixture.authorities().take(8) {
            let vote = authority.vote(cert.header());
            signatures.push((vote.author(), vote.signature().clone()));
        }

        cert_v1_round_2_missing.push(
            Certificate::new_unverified(
                &cert_v1_config,
                &committee,
                cert.header().clone(),
                signatures,
            )
            .unwrap(),
        );
    }
    let _ = tx_narwhal_round_updates.send(1);
    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone(),
        parents: cert_v1_round_2_missing.clone(),
    });
    assert!(request
        .extensions_mut()
        .insert(network.downgrade())
        .is_none());
    assert!(request
        .extensions_mut()
        .insert(anemo::PeerId(author.network_public_key().0.to_bytes()))
        .is_none());

    let result = timeout(Duration::from_secs(5), handler.request_vote(request))
        .await
        .unwrap();
    assert!(result.is_err(), "{:?}", result);

    // TEST PHASE 2: Handler should process missing parent certificates and succeed.
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

    let result = timeout(Duration::from_secs(5), handler.request_vote(request))
        .await
        .unwrap();
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
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        authority_id,
        fixture.committee(),
        latest_protocol_version(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id,
        committee: fixture.committee(),
        protocol_config: latest_protocol_version(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != authority_id) {
        let header: Header = primary
            .header_builder(&latest_protocol_version(), &fixture.committee())
            .with_payload_batch(
                test_utils::fixture_batch_with_transactions(
                    10,
                    &test_utils::latest_protocol_version(),
                ),
                0,
                0,
            )
            .build()
            .unwrap()
            .into();

        let certificate = fixture.certificate(&latest_protocol_version(), &header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());
        certificate_store.write(certificate.clone()).unwrap();
        for (digest, (worker_id, _)) in certificate.header().payload() {
            payload_store.write(digest, worker_id).unwrap();
        }
    }
    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            1,
            0,
        )
        .build()
        .unwrap()
        .into();
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
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        latest_protocol_version(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        protocol_config: latest_protocol_version(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let header: Header = primary
            .header_builder(&latest_protocol_version(), &fixture.committee())
            .with_payload_batch(
                test_utils::fixture_batch_with_transactions(
                    10,
                    &test_utils::latest_protocol_version(),
                ),
                0,
                0,
            )
            .build()
            .unwrap()
            .into();

        let certificate = fixture.certificate(&latest_protocol_version(), &header);
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
    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            1,
            0,
        )
        .build()
        .unwrap()
        .into();
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
    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            1,
            0,
        )
        .build()
        .unwrap()
        .into();
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

// TODO: Remove after network has moved to CertificateV2
#[tokio::test]
async fn test_fetch_certificates_v1_handler() {
    let cert_v1_protocol_config = get_protocol_config(28);
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let id = fixture.authorities().next().unwrap().id();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        cert_v1_protocol_config.clone(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates.clone(),
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        protocol_config: cert_v1_protocol_config.clone(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    let mut current_round: Vec<_> =
        Certificate::genesis(&cert_v1_protocol_config, &fixture.committee())
            .into_iter()
            .map(|cert| cert.header().clone())
            .collect();
    let mut headers = vec![];
    let total_rounds = 4;
    for i in 0..total_rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| {
                fixture
                    .certificate(&cert_v1_protocol_config, &header)
                    .digest()
            })
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents, &cert_v1_protocol_config);
        headers.extend(current_round.clone());
    }

    let total_authorities = fixture.authorities().count();
    let total_certificates = total_authorities * total_rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&cert_v1_protocol_config, &header));
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
async fn test_fetch_certificates_v2_handler() {
    let cert_v2_config = latest_protocol_version();
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let id = fixture.authorities().next().unwrap().id();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        cert_v2_config.clone(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates.clone(),
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        protocol_config: cert_v2_config.clone(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    let mut current_round: Vec<_> = Certificate::genesis(&cert_v2_config, &fixture.committee())
        .into_iter()
        .map(|cert| cert.header().clone())
        .collect();
    let mut headers = vec![];
    let total_rounds = 4;
    for i in 0..total_rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&cert_v2_config, &header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents, &cert_v2_config);
        headers.extend(current_round.clone());
    }

    let total_authorities = fixture.authorities().count();
    let total_certificates = total_authorities * total_rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&cert_v2_config, &header));
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
            let mut cert = certificates[i + j * total_authorities].clone();
            assert_eq!(&cert.header().author(), authorities.last().unwrap());
            if i == 3 && j == 3 {
                // Simulating only 1 directly verified certificate (Auth 3 Round 4) being stored.
                cert.set_signature_verification_state(
                    SignatureVerificationState::VerifiedDirectly(
                        cert.aggregated_signature()
                            .expect("Invalid Signature")
                            .clone(),
                    ),
                );
            } else {
                // Simulating some indirectly verified certificates being stored.
                cert.set_signature_verification_state(
                    SignatureVerificationState::VerifiedIndirectly(
                        cert.aggregated_signature()
                            .expect("Invalid Signature")
                            .clone(),
                    ),
                );
            }
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
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());

    let (certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_fetcher, _rx_certificate_fetcher) = test_utils::test_channel!(1);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(100);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(1, 0));
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(1u64);
    let (tx_randomness_partial_signatures, _rx_randomness_partial_signatures) =
        test_utils::test_channel!(1);
    let randomness_vss_key_lock = Arc::new(OnceLock::new());

    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        latest_protocol_version(),
        worker_cache.clone(),
        /* gc_depth */ 50,
        client.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates,
        tx_parents,
        rx_consensus_round_updates,
        metrics.clone(),
        &primary_channel_metrics,
    ));
    let handler = PrimaryReceiverHandler {
        authority_id: id,
        committee: fixture.committee(),
        protocol_config: latest_protocol_version(),
        worker_cache: worker_cache.clone(),
        synchronizer: synchronizer.clone(),
        signature_service,
        certificate_store: certificate_store.clone(),
        vote_digest_store: VoteDigestStore::new_for_tests(),
        rx_narwhal_round_updates,
        randomness_vss_key_lock,
        tx_randomness_partial_signatures,
        parent_digests: Default::default(),
        metrics: metrics.clone(),
    };

    // Make some mock certificates that are parents of our new header.
    let mut certificates = HashMap::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let header: Header = primary
            .header_builder(&latest_protocol_version(), &fixture.committee())
            .with_payload_batch(
                test_utils::fixture_batch_with_transactions(
                    10,
                    &test_utils::latest_protocol_version(),
                ),
                0,
                0,
            )
            .build()
            .unwrap()
            .into();

        let certificate = fixture.certificate(&latest_protocol_version(), &header);
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

    let test_header: Header = author
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            1,
            0,
        )
        .created_at(created_at)
        .build()
        .unwrap()
        .into();

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
        .header_builder(&latest_protocol_version(), &fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(
            test_utils::fixture_batch_with_transactions(10, &test_utils::latest_protocol_version()),
            1,
            0,
        )
        .created_at(created_at)
        .build()
        .unwrap();

    let mut request = anemo::Request::new(RequestVoteRequest {
        header: test_header.clone().into(),
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
