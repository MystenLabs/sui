// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        responses::PayloadAvailabilityResponse, BlockSynchronizer, CertificatesResponse, Command,
        PendingIdentifier, RequestID, SyncError,
    },
    common::{create_db_stores, worker_listener},
    primary::PrimaryMessage,
    PrimaryWorkerMessage,
};
use bincode::deserialize;
use config::BlockSynchronizerParameters;
use crypto::{ed25519::Ed25519PublicKey, Hash};
use futures::{future::try_join_all, stream::FuturesUnordered};
use network::{PrimaryNetwork, PrimaryToWorkerNetwork};
use serde::de::DeserializeOwned;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use test_utils::{
    certificate, fixture_batch_with_transactions, fixture_header_builder, keys,
    resolve_name_and_committee, PrimaryToPrimaryMockServer,
};
use tokio::{
    sync::{mpsc::channel, watch},
    task::JoinHandle,
    time::{sleep, timeout},
};
use types::ReconfigureNotification;

use crypto::traits::KeyPair;
use tracing::debug;
use types::{Certificate, CertificateDigest};

#[tokio::test]
async fn test_successful_headers_synchronization() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();

    // AND the necessary keys
    let (name, committee) = resolve_name_and_committee();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(10);
    let (tx_certificate_responses, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();

    let key = keys(None).pop().unwrap();
    let worker_id_0 = 0;
    let worker_id_1 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch_1.clone(), worker_id_0)
            .with_payload_batch(batch_2.clone(), worker_id_1)
            .build(&key)
            .unwrap();

        let certificate = certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    // AND create the synchronizer
    BlockSynchronizer::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        PrimaryNetwork::default(),
        payload_store.clone(),
        certificate_store.clone(),
        BlockSynchronizerParameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let handlers: FuturesUnordered<JoinHandle<Vec<PrimaryMessage<Ed25519PublicKey>>>> = committee
        .others_primaries(&name)
        .into_iter()
        .map(|primary| {
            println!("New primary added: {:?}", primary.1.primary_to_primary);
            primary_listener::<PrimaryMessage<Ed25519PublicKey>>(1, primary.1.primary_to_primary)
        })
        .collect();

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
                    let response_certificates: Vec<(
                        CertificateDigest,
                        Option<Certificate<Ed25519PublicKey>>,
                    )> = certificate_ids
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

                    tx_certificate_responses
                        .send(CertificatesResponse {
                            certificates: response_certificates,
                            from: primaries.pop().unwrap().0,
                        })
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
    let (name, committee) = resolve_name_and_committee();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(10);
    let (_tx_certificate_responses, rx_certificate_responses) = channel(10);
    let (tx_payload_availability_responses, rx_payload_availability_responses) = channel(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();

    let key = keys(None).pop().unwrap();
    let worker_id_0: u32 = 0;
    let worker_id_1: u32 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..8 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch_1.clone(), worker_id_0)
            .with_payload_batch(batch_2.clone(), worker_id_1)
            .build(&key)
            .unwrap();

        let certificate = certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    // AND create the synchronizer
    BlockSynchronizer::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        PrimaryNetwork::default(),
        payload_store.clone(),
        certificate_store.clone(),
        BlockSynchronizerParameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = channel(10);

    // AND let's assume that all the primaries are responding with the full set
    // of requested certificates.
    let handlers_primaries: FuturesUnordered<JoinHandle<Vec<PrimaryMessage<Ed25519PublicKey>>>> =
        committee
            .others_primaries(&name)
            .into_iter()
            .map(|primary| {
                println!("New primary added: {:?}", primary.1.primary_to_primary);
                primary_listener::<PrimaryMessage<Ed25519PublicKey>>(
                    1,
                    primary.1.primary_to_primary,
                )
            })
            .collect();

    // AND spin up the corresponding worker nodes
    let mut workers = vec![
        (worker_id_0, committee.worker(&name, &worker_id_0).unwrap()),
        (worker_id_1, committee.worker(&name, &worker_id_1).unwrap()),
    ];

    let handlers_workers: FuturesUnordered<
        JoinHandle<Vec<PrimaryWorkerMessage<Ed25519PublicKey>>>,
    > = workers
        .iter()
        .map(|worker| {
            println!("New worker added: {:?}", worker.1.primary_to_worker);
            worker_listener::<PrimaryWorkerMessage<Ed25519PublicKey>>(
                -1,
                worker.1.primary_to_worker.clone(),
            )
        })
        .collect();

    // WHEN
    tx_commands
        .send(Command::SynchronizeBlockPayload {
            certificates: certificates.values().cloned().collect(),
            respond_to: tx_synchronize,
        })
        .await
        .ok()
        .unwrap();

    // wait for the primaries to receive all the requests
    if let Ok(result) = timeout(
        Duration::from_millis(4_000),
        try_join_all(handlers_primaries),
    )
    .await
    {
        assert!(result.is_ok(), "Error returned");

        let mut primaries = committee.others_primaries(&name);

        for mut primary_responses in result.unwrap() {
            // ensure that only one request has been received
            assert_eq!(primary_responses.len(), 1, "Expected only one request");

            match primary_responses.remove(0) {
                PrimaryMessage::PayloadAvailabilityRequest {
                    certificate_ids,
                    requestor,
                } => {
                    let response: Vec<(CertificateDigest, bool)> = certificate_ids
                        .iter()
                        .map(|id| (*id, certificates.contains_key(id)))
                        .collect();

                    debug!("{:?}", requestor);

                    tx_payload_availability_responses
                        .send(PayloadAvailabilityResponse {
                            block_ids: response,
                            from: primaries.pop().unwrap().0,
                        })
                        .await
                        .unwrap();
                }
                _ => {
                    panic!("Unexpected request has been received!");
                }
            }
        }
    }

    // now wait to receive all the requests from the workers
    if let Ok(result) = timeout(Duration::from_millis(4_000), try_join_all(handlers_workers)).await
    {
        assert!(result.is_ok(), "Error returned");

        for messages in result.unwrap() {
            // since everything is in order, just pop the next worker
            let worker = workers.pop().unwrap();

            for m in messages {
                match m {
                    PrimaryWorkerMessage::Synchronize(batch_ids, _) => {
                        // Assume that the request is the correct one and just immediately
                        // store the batch to the payload store.
                        for batch_id in batch_ids {
                            payload_store.write((batch_id, worker.0), 1).await;
                        }
                    }
                    _ => {
                        panic!("Unexpected request received");
                    }
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
    let (name, committee) = resolve_name_and_committee();

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(10);
    let (_, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    // AND some blocks (certificates)
    let mut certificates: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();

    let key = keys(None).pop().unwrap();

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _ in 0..5 {
        let header = fixture_header_builder()
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(&key)
            .unwrap();

        let certificate = certificate(&header);

        certificates.insert(certificate.clone().digest(), certificate.clone());
    }

    let mut block_ids: Vec<CertificateDigest> = certificates.keys().copied().collect();

    let mut block_synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        pending_requests: HashMap::new(),
        map_certificate_responses_senders: HashMap::new(),
        map_payload_availability_responses_senders: HashMap::new(),
        primary_network: PrimaryNetwork::default(),
        worker_network: PrimaryToWorkerNetwork::default(),
        payload_store,
        certificate_store,
        certificates_synchronize_timeout: Duration::from_millis(2_000),
        payload_synchronize_timeout: Duration::from_millis(2_000),
        payload_availability_timeout: Duration::from_millis(2_000),
    };

    // ResultSender
    let get_mock_sender = || {
        let (tx, _) = channel(10);
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
    let request_id: RequestID = block_ids.iter().collect();

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

    let request_id: RequestID = vec![extra_certificate_id].iter().collect();
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
    let (name, committee) = resolve_name_and_committee();
    let key = keys(None).pop().unwrap();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(10);
    let (_, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    // AND some random block ids
    let block_ids: Vec<CertificateDigest> = (0..10)
        .into_iter()
        .map(|_| {
            let header = fixture_header_builder()
                .with_payload_batch(fixture_batch_with_transactions(10), 0)
                .build(&key)
                .unwrap();

            certificate(&header).digest()
        })
        .collect();

    // AND create the synchronizer
    BlockSynchronizer::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        PrimaryNetwork::default(),
        payload_store.clone(),
        certificate_store.clone(),
        BlockSynchronizerParameters::default(),
    );

    // AND the channel to respond to
    let (tx_synchronize, mut rx_synchronize) = channel(10);

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
    let (name, committee) = resolve_name_and_committee();
    let key = keys(None).pop().unwrap();

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(10);
    let (_, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    let synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        map_payload_availability_responses_senders: Default::default(),
        primary_network: Default::default(),
        worker_network: Default::default(),
        certificate_store: certificate_store.clone(),
        payload_store,
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();
    let mut block_ids = Vec::new();
    const NUM_OF_MISSING_CERTIFICATES: u32 = 5;

    // AND storing some certificates
    for i in 1..=8 {
        let batch = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch.clone(), 0)
            .build(&key)
            .unwrap();

        let certificate = certificate(&header);

        block_ids.push(certificate.digest());
        certificates.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_MISSING_CERTIFICATES {
            certificate_store
                .write(certificate.digest(), certificate)
                .await;
        }
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = channel(10);

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
    let (name, committee) = resolve_name_and_committee();
    let key = keys(None).pop().unwrap();

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(10);
    let (_, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    let synchronizer = BlockSynchronizer {
        name,
        committee: committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        map_payload_availability_responses_senders: Default::default(),
        primary_network: Default::default(),
        worker_network: Default::default(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates_map: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();
    let mut certificates = Vec::new();
    const NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD: u32 = 5;

    // AND storing some certificates
    for i in 1..=8 {
        let batch = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch.clone(), 0)
            .build(&key)
            .unwrap();

        let certificate: Certificate<Ed25519PublicKey> = certificate(&header);

        certificates.push(certificate.clone());
        certificates_map.insert(certificate.clone().digest(), certificate.clone());

        if i > NUM_OF_CERTIFICATES_WITH_MISSING_PAYLOAD {
            certificate_store
                .write(certificate.digest(), certificate.clone())
                .await;

            for entry in certificate.header.payload {
                payload_store.write(entry, 1).await;
            }
        }
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = channel(10);

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
    let (_, committee) = resolve_name_and_committee();
    let key = keys(None).pop().unwrap();

    // AND make sure the key used for our "own" primary is the one that will
    // be used to create the headers.
    let name = key.public().clone();

    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(10);
    let (_, rx_certificate_responses) = channel(10);
    let (_, rx_payload_availability_responses) = channel(10);

    let synchronizer = BlockSynchronizer {
        name: name.clone(),
        committee: committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_certificate_responses,
        rx_payload_availability_responses,
        pending_requests: Default::default(),
        map_certificate_responses_senders: Default::default(),
        map_payload_availability_responses_senders: Default::default(),
        primary_network: Default::default(),
        worker_network: Default::default(),
        certificate_store: certificate_store.clone(),
        payload_store: payload_store.clone(),
        certificates_synchronize_timeout: Default::default(),
        payload_synchronize_timeout: Default::default(),
        payload_availability_timeout: Default::default(),
    };

    let mut certificates_map: HashMap<CertificateDigest, Certificate<Ed25519PublicKey>> =
        HashMap::new();
    let mut certificates = Vec::new();

    // AND storing some certificates
    for _ in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let builder = types::HeaderBuilder::<Ed25519PublicKey>::default();
        let header = builder
            .author(name.clone())
            .round(1)
            .epoch(0)
            .parents(
                Certificate::genesis(&committee)
                    .iter()
                    .map(|x| x.digest())
                    .collect(),
            )
            .with_payload_batch(batch.clone(), 0)
            .build(&key)
            .unwrap();

        let certificate: Certificate<Ed25519PublicKey> = certificate(&header);

        certificates.push(certificate.clone());
        certificates_map.insert(certificate.clone().digest(), certificate.clone());
    }

    // AND create a dummy sender/receiver
    let (tx, mut rx) = channel(10);

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

pub fn primary_listener<T>(
    num_of_expected_responses: i32,
    address: multiaddr::Multiaddr,
) -> JoinHandle<Vec<T>>
where
    T: Send + DeserializeOwned + 'static,
{
    tokio::spawn(async move {
        let mut recv = PrimaryToPrimaryMockServer::spawn(address);
        let mut responses = Vec::new();

        loop {
            let message = recv
                .recv()
                .await
                .expect("Failed to receive network message");
            match deserialize::<'_, T>(&message.payload) {
                Ok(msg) => {
                    responses.push(msg);

                    // if -1 is given, then we don't count the number of messages
                    // but we just rely to receive as many as possible until timeout
                    // happens when waiting for requests.
                    if num_of_expected_responses != -1
                        && responses.len() as i32 == num_of_expected_responses
                    {
                        return responses;
                    }
                }
                Err(err) => {
                    panic!("Error occurred {err}");
                }
            }
        }
    })
}
