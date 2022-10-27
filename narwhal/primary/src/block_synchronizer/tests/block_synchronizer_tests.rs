// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        responses::AvailabilityResponse, BlockSynchronizer, CertificatesResponse, Command,
        PendingIdentifier, RequestID, SyncError,
    },
    common::{create_db_stores, worker_listener},
    primary::PrimaryMessage,
};
use anemo::{types::PeerInfo, PeerId};
use config::{BlockSynchronizerParameters, Parameters};
use fastcrypto::hash::Hash;
use futures::{future::try_join_all, stream::FuturesUnordered};
use network::P2pNetwork;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use test_utils::{fixture_batch_with_transactions, CommitteeFixture, PrimaryToPrimaryMockServer};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{sleep, timeout},
};
use types::{
    MockPrimaryToPrimary, PayloadAvailabilityResponse, PrimaryToPrimaryServer,
    ReconfigureNotification,
};

use crypto::NetworkKeyPair;
use fastcrypto::traits::KeyPair as _;
use tracing::debug;
use types::{Certificate, CertificateDigest};

#[tokio::test]
async fn test_successful_headers_synchronization() {
    telemetry_subscribers::init_for_testing();
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (tx_availability_responses, rx_availability_responses) = test_utils::test_channel!(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();

    let worker_id_0 = 0;
    let worker_id_1 = 1;

    // TODO: duplicated code in this file.
    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch_1.clone(), worker_id_0)
            .with_payload_batch(batch_2.clone(), worker_id_1)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // TODO: quite duplicated in our repo (3 times).
    for (_pubkey, address, network_pubkey) in committee.others_primaries(&name) {
        let peer_id = PeerId(network_pubkey.0.to_bytes());
        let address = network::multiaddr_to_address(&address).unwrap();
        let peer_info = PeerInfo {
            peer_id,
            affinity: anemo::types::PeerAffinity::High,
            address: vec![address],
        };
        network.known_peers().insert(peer_info);
    }

    // AND create the synchronizer
    let _synchronizer_handle = BlockSynchronizer::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        P2pNetwork::new(network.clone()),
        payload_store.clone(),
        certificate_store.clone(),
        Parameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let handlers: FuturesUnordered<JoinHandle<Vec<PrimaryMessage>>> = fixture
        .authorities()
        .filter(|a| a.public_key() != name)
        .map(|a| {
            let address = committee.primary(&a.public_key()).unwrap();
            println!("New primary added: {:?}", address);
            primary_listener(1, a.network_keypair().copy(), address)
        })
        .collect();

    // Wait for connectivity
    let (mut events, mut peers) = network.subscribe();
    while peers.len() != 3 {
        let event = events.recv().await.unwrap();
        match event {
            anemo::types::PeerEvent::NewPeer(peer_id) => peers.push(peer_id),
            anemo::types::PeerEvent::LostPeer(_, _) => {
                panic!("we shouldn't see any lost peer events")
            }
        }
    }

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockHeaders {
            block_ids: certificates.keys().copied().collect(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // wait for the primaries to receive all the requests
    if let Ok(result) = timeout(Duration::from_millis(4_000), try_join_all(handlers)).await {
        assert!(result.is_ok(), "Error returned");

        let mut primaries = committee.others_primaries(&name);

        for mut primary_responses in result.unwrap() {
            // ensure that only one request has been received
            assert_eq!(primary_responses.len(), 1, "Expected only one request");

            match primary_responses.remove(0) {
                PrimaryMessage::CertificatesBatchRequest {
                    certificate_ids,
                    requestor,
                } => {
                    let response_certificates: Vec<(CertificateDigest, Option<Certificate>)> =
                        certificate_ids
                            .iter()
                            .map(|id| {
                                if let Some(certificate) = certificates.get(id) {
                                    (*id, Some(certificate.clone()))
                                } else {
                                    panic!(
                                    "Received certificate with id {id} not amongst the expected"
                                );
                                }
                            })
                            .collect();

                    debug!("{:?}", requestor);

                    tx_availability_responses
                        .send(AvailabilityResponse::Certificate(CertificatesResponse {
                            certificates: response_certificates,
                            from: primaries.pop().unwrap().0,
                        }))
                        .await
                        .unwrap();
                }
                _ => {
                    panic!("Unexpected request has been received!");
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
async fn test_successful_payload_synchronization() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_tx_availability_responses, rx_availability_responses) = test_utils::test_channel!(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();

    let worker_id_0: u32 = 0;
    let worker_id_1: u32 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch_1.clone(), worker_id_0)
            .with_payload_batch(batch_2.clone(), worker_id_1)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    // AND create the synchronizer
    let _synchronizer_handle = BlockSynchronizer::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        P2pNetwork::new(network.clone()),
        payload_store.clone(),
        certificate_store.clone(),
        Parameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let mut primary_networks = Vec::new();
    for primary in fixture.authorities().filter(|a| a.public_key() != name) {
        let address = committee.primary(&primary.public_key()).unwrap();
        let certificates = certificates.clone();
        let mut mock_server = MockPrimaryToPrimary::new();
        mock_server
            .expect_get_payload_availability()
            .returning(move |request| {
                Ok(anemo::Response::new(PayloadAvailabilityResponse {
                    payload_availability: request
                        .body()
                        .certificate_ids
                        .iter()
                        .map(|id| (*id, certificates.contains_key(id)))
                        .collect(),
                }))
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(mock_server));
        primary_networks.push(primary.new_network(routes));
        println!("New primary added: {:?}", address);

        let address = network::multiaddr_to_address(&address).unwrap();
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

        let address = network::multiaddr_to_address(worker_address).unwrap();
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

        for ((_primary_messages, sync_messages), worker) in
            result.unwrap().into_iter().zip(workers.into_iter())
        {
            for m in sync_messages {
                // Assume that the request is the correct one and just immediately
                // store the batch to the payload store.
                for batch_id in m.digests {
                    payload_store.write((batch_id, worker), 1).await;
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
async fn test_multiple_overlapping_requests() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_, rx_availability_responses) = test_utils::test_channel!(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..5 {
        let header = author
            .header_builder(&committee)
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let mut block_ids: Vec<CertificateDigest> = certificates.keys().copied().collect();

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    println!("New primary added: {:?}", own_address);
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    let mut block_synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        pending_requests: HashMap::new(),
        map_certificate_responses_senders: HashMap::new(),
        network: P2pNetwork::new(network),
        payload_store,
        certificate_store,
        certificates_synchronize_timeout: Duration::from_secs(1),
        payload_synchronize_timeout: Duration::from_secs(1),
        payload_availability_timeout: Duration::from_secs(1),
    };

    // ResultSender
    let get_mock_sender = || {
        let (tx, _) = mpsc::channel(10);
        tx
    };

    // WHEN
    let result = block_synchronizer
        .handle_synchronize_block_headers_command(block_ids.clone(), get_mock_sender())
        .await;
    assert!(
        result.is_some(),
        "Should have created a future to fetch certificates"
    );

    // THEN

    // ensure that pending values have been updated
    for digest in block_ids.clone() {
        assert!(
            block_synchronizer
                .pending_requests
                .contains_key(&PendingIdentifier::Header(digest)),
            "Expected to have certificate {} pending to retrieve",
            digest
        );
    }

    // AND that the request is pending for all the block_ids
    let request_id: RequestID = block_ids.clone().into_iter().collect();

    assert!(
        block_synchronizer
            .map_certificate_responses_senders
            .contains_key(&request_id),
        "Expected to have a request for request id {:?}",
        &request_id
    );

    // AND when trying to request same block ids + extra
    let extra_certificate_id = CertificateDigest::default();
    block_ids.push(extra_certificate_id);
    let result = block_synchronizer
        .handle_synchronize_block_headers_command(block_ids, get_mock_sender())
        .await;
    assert!(
        result.is_some(),
        "Should have created a future to fetch certificates"
    );

    // THEN only the extra id will be requested
    assert_eq!(
        block_synchronizer.map_certificate_responses_senders.len(),
        2
    );

    let request_id: RequestID = vec![extra_certificate_id].into_iter().collect();
    assert!(
        block_synchronizer
            .map_certificate_responses_senders
            .contains_key(&request_id),
        "Expected to have a request for request id {}",
        &request_id
    );
}

#[tokio::test]
async fn test_timeout_while_waiting_for_certificates() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_commands, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_, rx_availability_responses) = test_utils::test_channel!(10);

    // AND some random block ids
    let block_ids: Vec<CertificateDigest> = (0..10)
        .into_iter()
        .map(|_| {
            let header = author
                .header_builder(&committee)
                .with_payload_batch(fixture_batch_with_transactions(10), 0)
                .build(author.keypair())
                .unwrap();

            fixture.certificate(&header).digest()
        })
        .collect();

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
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
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        P2pNetwork::new(network),
        payload_store.clone(),
        certificate_store.clone(),
        params.clone(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = mpsc::channel(10);

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockHeaders {
            block_ids: block_ids.clone(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // THEN
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    let mut total_results_received = 0;

    let mut block_ids_seen: HashSet<CertificateDigest> = HashSet::new();

    loop {
        tokio::select! {
            Some(result) = rx_synchronize.recv() => {
                assert!(result.is_err(), "Expected error result, instead received: {:?}", result.unwrap());

                match result.err().unwrap() {
                    SyncError::Timeout { block_id } => {
                        // ensure the results are unique and within the expected set
                        assert!(block_ids_seen.insert(block_id), "Already received response for this block id - this shouldn't happen");
                        assert!(block_ids.iter().any(|d|d.eq(&block_id)), "Received not expected block id");
                    },
                    err => panic!("Didn't expect this sync error: {:?}", err)
                }

                total_results_received += 1;

                // received all expected results, now break
                if total_results_received == block_ids.as_slice().len() {
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
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_, rx_availability_responses) = test_utils::test_channel!(10);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();

    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    let synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        network: P2pNetwork::new(network),
        certificate_store: certificate_store.clone(),
        payload_store,
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates: HashMap<CertificateDigest, Certificate> = HashMap::new();
    let mut block_ids = Vec::new();
    const NUM_OF_MISSING_CERTIFICATES: u32 = 5;

    // AND storing some certificates
    for i in 1..=8 {
        let batch = fixture_batch_with_transactions(10);

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch.clone(), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);

        block_ids.push(certificate.digest());
        certificates.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_MISSING_CERTIFICATES {
            certificate_store.write(certificate).unwrap();
        }
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = mpsc::channel(10);

    // WHEN
    let missing_certificates = synchronizer
        .reply_with_certificates_already_in_storage(block_ids, tx)
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
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_, rx_availability_responses) = test_utils::test_channel!(10);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();

    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    let synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        network: P2pNetwork::new(network),
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

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch.clone(), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);

        certificates.push(certificate.clone());
        certificates_map.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD {
            certificate_store.write(certificate.clone()).unwrap();

            for entry in certificate.header.payload {
                payload_store.write(entry, 1).await;
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
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();

    // AND make sure the key used for our "own" primary is the one that will
    // be used to create the headers.
    let name = primary.public_key();

    let (_, rx_reconfigure) = watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (_, rx_block_synchronizer_commands) = test_utils::test_channel!(10);
    let (_, rx_availability_responses) = test_utils::test_channel!(10);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();

    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();
    let synchronizer = BlockSynchronizer {
        name: name.clone(),
        committee: committee.clone(),
        worker_cache: worker_cache.clone(),
        rx_reconfigure,
        rx_block_synchronizer_commands,
        rx_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        network: P2pNetwork::new(network),
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

        let header = primary
            .header_builder(&committee)
            .with_payload_batch(batch.clone(), 0)
            .build(primary.keypair())
            .unwrap();

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

#[must_use]
fn primary_listener(
    num_of_expected_responses: i32,
    network_keypair: NetworkKeyPair,
    address: multiaddr::Multiaddr,
) -> JoinHandle<Vec<PrimaryMessage>> {
    tokio::spawn(async move {
        let (mut recv, _network) = PrimaryToPrimaryMockServer::spawn(network_keypair, address);
        let mut responses = Vec::new();

        loop {
            let message = recv
                .recv()
                .await
                .expect("Failed to receive network message");
            responses.push(message);

            // if -1 is given, then we don't count the number of messages
            // but we just rely to receive as many as possible until timeout
            // happens when waiting for requests.
            if num_of_expected_responses != -1
                && responses.len() as i32 == num_of_expected_responses
            {
                return responses;
            }
        }
    })
}
