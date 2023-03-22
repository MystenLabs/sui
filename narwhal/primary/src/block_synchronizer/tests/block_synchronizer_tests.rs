// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{BlockSynchronizer, Command, SyncError},
    common::{create_db_stores, worker_listener},
    NUM_SHUTDOWN_RECEIVERS,
};
use anemo::PeerId;
use config::{BlockSynchronizerParameters, Parameters};
use fastcrypto::hash::Hash;
use futures::future::try_join_all;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use test_utils::{fixture_batch_with_transactions, CommitteeFixture};
use tokio::{
    sync::mpsc,
    time::{sleep, timeout},
};
use types::{
    CertificateAPI, GetCertificatesResponse, Header, HeaderAPI, MockPrimaryToPrimary,
    PayloadAvailabilityResponse, PreSubscribedBroadcastSender, PrimaryToPrimaryServer,
};

use fastcrypto::traits::KeyPair as _;

use types::{Certificate, CertificateDigest};

#[tokio::test]
async fn test_successful_headers_synchronization() {
    telemetry_subscribers::init_for_testing();
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();

    let worker_id_0 = 0;
    let worker_id_1 = 1;

    // TODO: duplicated code in this file.
    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch_1.clone(), worker_id_0, 0)
                .with_payload_batch(batch_2.clone(), worker_id_1, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // AND create the synchronizer
    let _synchronizer_handle = BlockSynchronizer::spawn(
        id,
        committee.clone(),
        worker_cache.clone(),
        tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        network.clone(),
        payload_store.clone(),
        certificate_store.clone(),
        Parameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let certificates = certificates.clone();
        let mut mock_server = MockPrimaryToPrimary::new();
        mock_server
            .expect_get_certificates()
            .returning(move |request| {
                Ok(anemo::Response::new(GetCertificatesResponse {
                    certificates: request
                        .body()
                        .digests
                        .iter()
                        .filter_map(|digest| certificates.get(digest))
                        .cloned()
                        .collect(),
                }))
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(mock_server));
        primary_networks.push(primary.new_network(routes));
        println!("New primary added: {:?}", address);

        let address = address.to_anemo_address().unwrap();
        let peer_id = PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockHeaders {
            digests: certificates.keys().copied().collect(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // THEN
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    let total_expected_results = certificates.len();
    let mut total_results_received = 0;

    loop {
        tokio::select! {
            Some(result) = rx_synchronize.recv() => {
                assert!(result.is_ok(), "Error result received: {:?}", result.err().unwrap());

                if result.is_ok() {
                    let block_header = result.ok().unwrap();
                    let certificate = block_header.certificate;

                    println!("Received certificate result: {:?}", certificate.clone());

                    assert!(certificates.contains_key(&certificate.digest()));
                    assert!(!block_header.fetched_from_storage, "Didn't expect to have fetched certificate from storage");

                    total_results_received += 1;
                }

                if total_results_received == total_expected_results {
                    break;
                }
            },
            () = &mut timer => {
                panic!("Timeout, no result has been received in time")
            }
        }
    }
}

#[tokio::test]
async fn test_successful_payload_synchronization() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();

    let worker_id_0: u32 = 0;
    let worker_id_1: u32 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch_1.clone(), worker_id_0, 0)
                .with_payload_batch(batch_2.clone(), worker_id_1, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // AND create the synchronizer
    let _synchronizer_handle = BlockSynchronizer::spawn(
        id,
        committee.clone(),
        worker_cache.clone(),
        tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        network.clone(),
        payload_store.clone(),
        certificate_store.clone(),
        Parameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.id() != id) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let certificates = certificates.clone();
        let mut mock_server = MockPrimaryToPrimary::new();
        mock_server
            .expect_get_payload_availability()
            .returning(move |request| {
                Ok(anemo::Response::new(PayloadAvailabilityResponse {
                    payload_availability: request
                        .body()
                        .certificate_digests
                        .iter()
                        .map(|digest| (*digest, certificates.contains_key(digest)))
                        .collect(),
                }))
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(mock_server));
        primary_networks.push(primary.new_network(routes));
        println!("New primary added: {:?}", address);

        let address = address.to_anemo_address().unwrap();
        let peer_id = PeerId(primary.network_keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // AND spin up the corresponding worker nodes
    let workers = vec![worker_id_0, worker_id_1];

    let mut handlers_workers = Vec::new();
    for worker_id in &workers {
        let worker = primary.worker(*worker_id);
        let network_key = worker.keypair();
        let worker_name = network_key.public().clone();
        let worker_address = &worker.info().worker_address;

        println!("New worker added: {:?}", worker_name);
        let handler = worker_listener(-1, worker_address.clone(), network_key);
        handlers_workers.push(handler);

        let address = worker_address.to_anemo_address().unwrap();
        let peer_id = PeerId(worker_name.0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockPayload {
            certificates: certificates.values().cloned().collect(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // now wait to receive all the requests from the workers
    if let Ok(result) = timeout(Duration::from_millis(4_000), try_join_all(handlers_workers)).await
    {
        assert!(result.is_ok(), "Error returned");

        for (sync_messages, worker) in result.unwrap().into_iter().zip(workers.into_iter()) {
            for m in sync_messages {
                // Assume that the request is the correct one and just immediately
                // store the batch to the payload store.
                for digest in m.digests {
                    payload_store.write(&digest, &worker).unwrap();
                }
            }
        }
    }

    // THEN
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    let total_expected_results = certificates.len();
    let mut total_results_received = 0;

    loop {
        tokio::select! {
            Some(result) = rx_synchronize.recv() => {
                assert!(result.is_ok(), "Error result received: {:?}", result.err().unwrap());

                if result.is_ok() {
                    let block_header = result.ok().unwrap();
                    let certificate = block_header.certificate;

                    println!("Received certificate result: {:?}", certificate.clone());

                    assert!(certificates.contains_key(&certificate.digest()));
                    assert!(!block_header.fetched_from_storage, "Didn't expect to have fetched certificate from storage");

                    total_results_received += 1;
                }

                if total_results_received == total_expected_results {
                    break;
                }
            },
            () = &mut timer => {
                panic!("Timeout, no result has been received in time")
            }
        }
    }
}

#[tokio::test]
async fn test_timeout_while_waiting_for_certificates() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

    // AND some random block digests
    let digests: Vec<CertificateDigest> = (0..10)
        .map(|_| {
            let header = Header::V1(
                author
                    .header_builder(&committee)
                    .with_payload_batch(fixture_batch_with_transactions(10), 0, 0)
                    .build()
                    .unwrap(),
            );

            fixture.certificate(&header).digest()
        })
        .collect();

    let own_address = committee
        .primary_by_id(&id)
        .unwrap()
        .to_anemo_address()
        .unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // AND create the synchronizer
    let params = Parameters {
        block_synchronizer: BlockSynchronizerParameters {
            certificates_synchronize_timeout: Duration::from_secs(1),
            ..Default::default()
        },
        ..Default::default()
    };
    let _synchronizer_handle = BlockSynchronizer::spawn(
        id,
        committee.clone(),
        worker_cache.clone(),
        tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        network,
        payload_store.clone(),
        certificate_store.clone(),
        params.clone(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockHeaders {
            digests: digests.clone(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // THEN
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    let mut total_results_received = 0;

    let mut digests_seen: HashSet<CertificateDigest> = HashSet::new();

    loop {
        tokio::select! {
            Some(result) = rx_synchronize.recv() => {
                assert!(result.is_err(), "Expected error result, instead received: {:?}", result.unwrap());

                match result.err().unwrap() {
                    SyncError::Timeout { digest } => {
                        // ensure the results are unique and within the expected set
                        assert!(digests_seen.insert(digest), "Already received response for this block digest - this shouldn't happen");
                        assert!(digests.iter().any(|d|d.eq(&digest)), "Received not expected block digest");
                    },
                    err => panic!("Didn't expect this sync error: {:?}", err)
                }

                total_results_received += 1;

                // received all expected results, now break
                if total_results_received == digests.as_slice().len() {
                    break;
                }
            },
            () = &mut timer => {
                panic!("Timeout, no result has been received in time")
            }
        }
    }
}

#[tokio::test]
async fn test_reply_with_certificates_already_in_storage() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let authority_id = primary.id();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

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

    let synchronizer = BlockSynchronizer {
        authority_id,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_shutdown: tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        pending_requests: Default::default(),
        network,
        certificate_store: certificate_store.clone(),
        payload_store,
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();
    let mut digests = Vec::new();
    const NUM_OF_MISSING_CERTIFICATES: u32 = 5;

    // AND storing some certificates
    for i in 1..=8 {
        let batch = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch.clone(), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);

        digests.push(certificate.digest());
        certificates.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_MISSING_CERTIFICATES {
            certificate_store.write(certificate).unwrap();
        }
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = mpsc::channel(10);

    // WHEN
    let missing_certificates = synchronizer
        .reply_with_certificates_already_in_storage(digests, tx)
        .await;

    // THEN some missing certificates exist
    assert_eq!(
        missing_certificates.len() as u32,
        NUM_OF_MISSING_CERTIFICATES,
        "Number of expected missing certificates differ."
    );

    // TODO: duplicated in this file.
    // AND should have received all the block headers
    for _ in 0..8 - NUM_OF_MISSING_CERTIFICATES {
        let result = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();

        let block_header = result.unwrap();

        assert!(
            block_header.fetched_from_storage,
            "Should have been fetched from storage"
        );
        assert!(
            certificates.contains_key(&block_header.certificate.digest()),
            "Not found expected certificate"
        );
    }
}

#[tokio::test]
async fn test_reply_with_payload_already_in_storage() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

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
    let synchronizer = BlockSynchronizer {
        authority_id: id,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_shutdown: tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        pending_requests: Default::default(),
        network,
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates_map: HashMap<CertificateDigest, Certificate> = HashMap::new();
    let mut certificates = Vec::new();
    const NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD: u32 = 5;

    // AND storing some certificates
    for i in 1..=8 {
        let batch = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch.clone(), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);

        certificates.push(certificate.clone());
        certificates_map.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD {
            certificate_store.write(certificate.clone()).unwrap();

            for (digest, (worker_id, _)) in certificate.header().payload() {
                payload_store.write(digest, worker_id).unwrap();
            }
        }
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = mpsc::channel(10);

    // WHEN
    let missing_certificates = synchronizer
        .reply_with_payload_already_in_storage(certificates, tx)
        .await;

    // THEN some certificates with missing payload exist
    assert_eq!(
        missing_certificates.len() as u32,
        NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD,
        "Number of expected missing certificates differ."
    );

    // AND should have received all the block headers
    for _ in 0..8 - NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD {
        let result = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();

        let block_header = result.unwrap();

        assert!(
            block_header.fetched_from_storage,
            "Should have been fetched from storage"
        );
        assert!(
            certificates_map.contains_key(&block_header.certificate.digest()),
            "Not found expected certificate"
        );
    }
}

#[tokio::test]
async fn test_reply_with_payload_already_in_storage_for_own_certificates() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    // AND make sure the key used for our "own" primary is the one that will
    // be used to create the headers.
    let authority_id = primary.id();

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);

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
    let synchronizer = BlockSynchronizer {
        authority_id,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_shutdown: tx_shutdown.subscribe(),
        rx_block_synchronizer_commands,
        pending_requests: Default::default(),
        network,
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates_map: HashMap<CertificateDigest, Certificate> = HashMap::new();
    let mut certificates = Vec::new();

    // AND storing some certificates
    for _ in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = Header::V1(
            primary
                .header_builder(&committee)
                .with_payload_batch(batch.clone(), 0, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);

        certificates.push(certificate.clone());
        certificates_map.insert(certificate.clone().digest(), certificate.clone());
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = mpsc::channel(10);

    // WHEN
    let missing_certificates = synchronizer
        .reply_with_payload_already_in_storage(certificates, tx)
        .await;

    // THEN no certificates with missing payload should exist
    assert_eq!(
        missing_certificates.len() as u32,
        0,
        "Didn't expect missing certificates"
    );

    // AND should have received all the block headers
    for _ in 0..5 {
        let result = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();

        let block_header = result.unwrap();

        assert!(
            block_header.fetched_from_storage,
            "Should have been fetched from storage"
        );
        assert!(
            certificates_map.contains_key(&block_header.certificate.digest()),
            "Not found expected certificate"
        );
    }
}
