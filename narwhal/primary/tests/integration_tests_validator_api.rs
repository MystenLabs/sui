use arc_swap::ArcSwap;
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, Parameters, WorkerId};
use consensus::{dag::Dag, metrics::ConsensusMetrics};
use crypto::PublicKey;
use fastcrypto::{traits::KeyPair as _, Hash};
use indexmap::IndexMap;
use network::metrics::WorkerNetworkMetrics;
use node::NodeStorage;
use primary::{NetworkModel, PayloadToken, Primary, CHANNEL_CAPACITY};
use prometheus::Registry;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};
use storage::CertificateStore;
use store::Store;
use test_utils::{
    certificate, fixture_batch_with_transactions, make_optimal_certificates,
    make_optimal_signed_certificates, temp_dir, AuthorityFixture, CommitteeFixture,
};
use tokio::sync::watch;
use tonic::transport::Channel;
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, CertificateDigestProto,
    CollectionRetrievalResult, Empty, GetCollectionsRequest, Header, HeaderDigest,
    ReadCausalRequest, ReconfigureNotification, RemoveCollectionsRequest, RetrievalResult,
    SerializedBatchMessage, Transaction, ValidatorClient,
};
use worker::{
    metrics::{Metrics, WorkerChannelMetrics, WorkerEndpointMetrics, WorkerMetrics},
    Worker, WorkerMessage,
};

#[tokio::test]
async fn test_get_collections() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let author = fixture.authorities().last().unwrap();

    let name = author.public_key();
    let signer = author.keypair().copy();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let worker_id = 0;
    let mut header_ids = Vec::new();
    // Blocks/Collections
    let mut collection_ids = Vec::new();
    let mut missing_block = CertificateDigest::new([0; 32]);

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch.clone(), worker_id)
            .build(author.keypair())
            .unwrap();

        let certificate = certificate(&header);
        let block_id = certificate.digest();
        collection_ids.push(block_id);

        // Write the certificate
        store.certificate_store.write(certificate.clone()).unwrap();

        // Write the header
        store
            .header_store
            .write(header.clone().id, header.clone())
            .await;

        header_ids.push(header.clone().id);

        // Write the batches to payload store
        store
            .payload_store
            .write_all(vec![((batch.clone().digest(), worker_id), 0)])
            .await
            .expect("couldn't store batches");
        if n != 4 {
            // Add batches to the workers store
            let message = WorkerMessage::Batch(batch.clone());
            let serialized_batch = bincode::serialize(&message).unwrap();
            store
                .batch_store
                .write(batch.digest(), serialized_batch)
                .await;
        } else {
            missing_block = block_id;
        }
    }

    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands, rx_get_block_commands) = test_utils::test_get_block_commands!(1);
    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    Primary::spawn(
        name.clone(),
        signer,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */
        tx_get_block_commands,
        rx_get_block_commands,
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates, consensus_metrics).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    let registry = Registry::new();
    let metrics = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry)),
    };

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store.batch_store.clone(),
        metrics,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(parameters.clone());

    // Test get no collections
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: vec![],
    });

    let status = client.get_collections(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Attempted fetch of no collections!"));

    // Test get 1 collection
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: vec![collection_ids[0].into()],
    });
    let response = client.get_collections(request).await.unwrap();
    let actual_result = response.into_inner().result;

    assert_eq!(1, actual_result.len());

    assert!(matches!(
        actual_result[0].retrieval_result,
        Some(types::RetrievalResult::Collection(_))
    ));

    // Test get 5 collections
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: collection_ids.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.get_collections(request).await.unwrap();
    let actual_result = response.into_inner().result;

    assert_eq!(5, actual_result.len());

    // One batch was intentionally left missing from the worker batch store.
    // Assert 4 Batches are returned
    assert_eq!(
        4,
        actual_result
            .iter()
            .filter(|&r| matches!(
                r.retrieval_result,
                Some(types::RetrievalResult::Collection(_))
            ))
            .count()
    );

    // And 1 Error is returned
    let errors: Vec<&CollectionRetrievalResult> = actual_result
        .iter()
        .filter(|&r| matches!(r.retrieval_result, Some(types::RetrievalResult::Error(_))))
        .collect::<Vec<_>>();

    assert_eq!(1, errors.len());

    // And check missing collection id is correct
    let actual_missing_collection = match errors[0].retrieval_result.as_ref().unwrap() {
        types::RetrievalResult::Error(e) => e.id.as_ref(),
        _ => panic!("Should never hit this branch."),
    };

    assert_eq!(
        &CertificateDigestProto::from(missing_block),
        actual_missing_collection.unwrap()
    );
}

#[tokio::test]
async fn test_remove_collections() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let author = fixture.authorities().last().unwrap();

    let name = author.public_key();
    let signer = author.keypair().copy();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let worker_id = 0;
    let mut header_ids = Vec::new();
    // Blocks/Collections
    let mut collection_ids = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);
    // No need to populate genesis in the Dag

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = author
            .header_builder(&committee)
            .with_payload_batch(batch.clone(), worker_id)
            .build(author.keypair())
            .unwrap();

        let certificate = certificate(&header);
        let block_id = certificate.digest();
        collection_ids.push(block_id);

        // Write the certificate
        store.certificate_store.write(certificate.clone()).unwrap();
        dag.insert(certificate.clone()).await.unwrap();

        // Write the header
        store
            .header_store
            .write(header.clone().id, header.clone())
            .await;

        header_ids.push(header.clone().id);

        // Write the batches to payload store
        store
            .payload_store
            .write_all(vec![((batch.clone().digest(), worker_id), 0)])
            .await
            .expect("couldn't store batches");
        if n != 4 {
            // Add batches to the workers store
            let message = WorkerMessage::Batch(batch.clone());
            let serialized_batch = bincode::serialize(&message).unwrap();
            store
                .batch_store
                .write(batch.digest(), serialized_batch)
                .await;
        }
    }

    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands, rx_get_block_commands) = test_utils::test_get_block_commands!(1);
    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    Primary::spawn(
        name.clone(),
        signer,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        tx_get_block_commands,
        rx_get_block_commands,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(parameters.clone());

    // Test remove 1 collection without spawning worker. Should result in a timeout error
    // when trying to remove batches.
    let block_to_be_removed = collection_ids.remove(0);
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![block_to_be_removed.into()],
    });

    let status = client.remove_collections(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Timeout, no result has been received in time"));
    assert!(
        store
            .certificate_store
            .read(block_to_be_removed)
            .unwrap()
            .is_some(),
        "Certificate should still exist"
    );

    let registry = Registry::new();
    let metrics = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry)),
    };

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store.batch_store.clone(),
        metrics,
    );

    // Test remove no collections
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![],
    });

    let status = client.remove_collections(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Attempted to remove no collections!"));

    // Test remove 1 collection
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![block_to_be_removed.into()],
    });
    let response = client.remove_collections(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(Empty {}, actual_result);

    assert!(
        store
            .certificate_store
            .read(block_to_be_removed)
            .unwrap()
            .is_none(),
        "Certificate shouldn't exist"
    );

    // Test remove remaining collections, one collection has its batches intentionally
    // missing but it should not return any errors.
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: collection_ids.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.remove_collections(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(Empty {}, actual_result);

    assert!(
        store
            .certificate_store
            .read_all(collection_ids.clone())
            .unwrap()
            .iter()
            .filter(|c| c.is_some())
            .count()
            == 0,
        "Certificate shouldn't exist"
    );

    // Test removing collections again after they have all been removed, no error
    // returned.
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: collection_ids.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.remove_collections(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(Empty {}, actual_result);
}

#[tokio::test]
async fn test_read_causal_signed_certificates() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir());
    let primary_store_2: NodeStorage = NodeStorage::reopen(temp_dir());

    let mut collection_ids: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);

    // No need to  genesis in the Dag
    let genesis_certs = Certificate::genesis(&committee);

    // Write genesis certs to primary 1 & 2
    primary_store_1
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();
    primary_store_2
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();

    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let keys = fixture
        .authorities()
        .map(|a| a.keypair().copy())
        .collect::<Vec<_>>();
    let (certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

    collection_ids.extend(
        certificates
            .iter()
            .map(|c| c.digest())
            .collect::<Vec<CertificateDigest>>(),
    );

    // Feed the certificates to the Dag
    for certificate in certificates.clone() {
        dag.insert(certificate).await.unwrap();
    }

    // Write the certificates to Primary 1 but intentionally miss one certificate.
    primary_store_1
        .certificate_store
        .write_all(certificates.clone().into_iter().skip(1))
        .unwrap();

    // Write all certificates to Primary 2, so Primary 1 has a place to retrieve
    // missing certificate from.
    primary_store_2
        .certificate_store
        .write_all(certificates.clone())
        .unwrap();

    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = authority_1.keypair().copy();
    let name_1 = keypair_1.public().clone();

    let (tx_get_block_commands_1, rx_get_block_commands_1) =
        test_utils::test_get_block_commands!(1);

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        name_1.clone(),
        keypair_1,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        tx_get_block_commands_1,
        rx_get_block_commands_1,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    let (tx_new_certificates_2, rx_new_certificates_2) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands_2, rx_get_block_commands_2) =
        test_utils::test_get_block_commands!(1);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = authority_2.keypair().copy();
    let name_2 = keypair_2.public().clone();
    let consensus_metrics_2 = Arc::new(ConsensusMetrics::new(&Registry::new()));

    // Spawn Primary 2
    Primary::spawn(
        name_2.clone(),
        keypair_2,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.payload_store,
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        tx_get_block_commands_2,
        rx_get_block_commands_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, consensus_metrics_2).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback_2,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(primary_1_parameters.clone());

    // Test read causal for existing collection in Primary 1
    // Collection is from genesis aka round 0 so we expect BFT 1 + 0 * 4 vertices
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(genesis_certs[0].digest().into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test read causal for existing collection in Primary 1
    // Collection is from round 1 so we expect BFT 1 + 0 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(collection_ids[1].into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test read causal for existing optimal certificates (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices
    for certificate in certificates {
        if certificate.round() == 4 {
            let request = tonic::Request::new(ReadCausalRequest {
                collection_id: Some(certificate.digest().into()),
            });

            let response = client.read_causal(request).await.unwrap();
            assert_eq!(13, response.into_inner().collection_ids.len());
        }
    }

    // Test read causal for missing collection from Primary 1. Expect block synchronizer
    // to handle retrieving the missing collection from Primary 2 before completing the
    // request for read causal.
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(collection_ids[0].into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());
}

#[tokio::test]
async fn test_read_causal_unsigned_certificates() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = authority_1.keypair().copy();
    let name_1 = authority_1.public_key();

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = authority_2.keypair().copy();
    let name_2 = authority_2.public_key();

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir());
    let primary_store_2: NodeStorage = NodeStorage::reopen(temp_dir());

    let mut collection_ids: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands_1, rx_get_block_commands_1) =
        test_utils::test_get_block_commands!(1);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);

    // No need to genesis in the Dag
    let genesis_certs = Certificate::genesis(&committee);

    // Write genesis certs to primary 1 & 2
    primary_store_1
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();
    primary_store_2
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();

    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (certificates, _next_parents) = make_optimal_certificates(
        1..=4,
        &genesis,
        &committee
            .authorities
            .keys()
            .cloned()
            .collect::<Vec<PublicKey>>(),
    );

    collection_ids.extend(
        certificates
            .iter()
            .map(|c| c.digest())
            .collect::<Vec<CertificateDigest>>(),
    );

    // Feed the certificates to the Dag
    for certificate in certificates.clone() {
        dag.insert(certificate).await.unwrap();
    }

    // Write the certificates to Primary 1 but intentionally miss one certificate.
    primary_store_1
        .certificate_store
        .write_all(certificates.clone().into_iter().skip(1))
        .unwrap();

    // Write all certificates to Primary 2, so Primary 1 has a place to retrieve
    // missing certificate from.
    primary_store_2
        .certificate_store
        .write_all(certificates.clone())
        .unwrap();

    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        name_1.clone(),
        keypair_1,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        tx_get_block_commands_1,
        rx_get_block_commands_1,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    let (tx_new_certificates_2, rx_new_certificates_2) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands_2, rx_get_block_commands_2) =
        test_utils::test_get_block_commands!(1);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);
    let consensus_metrics_2 = Arc::new(ConsensusMetrics::new(&Registry::new()));

    // Spawn Primary 2
    Primary::spawn(
        name_2.clone(),
        keypair_2,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.payload_store,
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        tx_get_block_commands_2,
        rx_get_block_commands_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, consensus_metrics_2).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback_2,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(primary_1_parameters.clone());

    // Test read causal for existing collection in Primary 1
    // Collection is from genesis aka round 0 so we expect BFT 1 + 0 * 4 vertices
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(genesis_certs[0].digest().into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test read causal for existing collection in Primary 1
    // Collection is from round 1 so we expect BFT 1 + 0 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(collection_ids[1].into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test read causal for existing optimal certificates (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices
    for certificate in certificates {
        if certificate.round() == 4 {
            let request = tonic::Request::new(ReadCausalRequest {
                collection_id: Some(certificate.digest().into()),
            });

            let response = client.read_causal(request).await.unwrap();
            assert_eq!(13, response.into_inner().collection_ids.len());
        }
    }

    // Test read causal for missing collection from Primary 1. Expect block synchronizer
    // to handle retrieving the missing collections from Primary 2 before completing the
    // request for read causal. However because these certificates were not signed
    // they will not pass validation during fetch.
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(collection_ids[0].into()),
    });

    let status = client.read_causal(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Error when trying to synchronize block headers: BlockNotFound"));
}

/// Here we test the ability on our code to synchronize missing certificates
/// by requesting them from other peers. On this example we emulate 2 authorities
/// (2 separate primary nodes) where we store a certificate on each one. Then we
/// are requesting via the get_collections call to the primary 1 to fetch the
/// collections for both certificates. Since primary 1 knows only about the
/// certificate 1 we expect to sync with primary 2 to fetch the unknown
/// certificate 2 after it has been processed for causal completion & validation.
/// We also expect to synchronize the missing batches of the missing certificate
/// from primary 2. All in all the end goal is to:
/// * Primary 1 be able to retrieve both certificates 1 & 2 successfully
/// * Primary 1 be able to fetch the payload for certificates 1 & 2
#[tokio::test]
async fn test_get_collections_with_missing_certificates() {
    // GIVEN keys for two primary nodes
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores for the 2 primaries
    let store_primary_1 = NodeStorage::reopen(temp_dir());
    let store_primary_2 = NodeStorage::reopen(temp_dir());

    // The certificate_1 will be stored in primary 1
    let (certificate_1, batch_1) = fixture_certificate(
        authority_1,
        &committee,
        &fixture,
        store_primary_1.header_store.clone(),
        store_primary_1.certificate_store.clone(),
        store_primary_1.payload_store.clone(),
        store_primary_1.batch_store.clone(),
    )
    .await;

    // The certificate_2 will be stored in primary 2
    let (certificate_2, batch_2) = fixture_certificate(
        authority_2,
        &committee,
        &fixture,
        store_primary_2.header_store.clone(),
        store_primary_2.certificate_store.clone(),
        store_primary_2.payload_store.clone(),
        store_primary_2.batch_store.clone(),
    )
    .await;

    let name_1 = authority_1.public_key();
    let name_2 = authority_2.public_key();

    let worker_id = 0;

    // AND keep a map of batches and payload
    let mut batches_map = HashMap::new();
    batches_map.insert(certificate_1.digest(), batch_1);
    batches_map.insert(certificate_2.digest(), batch_2);

    let block_ids = vec![certificate_1.digest(), certificate_2.digest()];

    // Spawn the primary 1 (which will be the one that we'll interact with)
    let (tx_new_certificates_1, rx_new_certificates_1) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_1, rx_feedback_1) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands_1, rx_get_block_commands_1) =
        test_utils::test_get_block_commands!(1);
    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    Primary::spawn(
        name_1.clone(),
        authority_1.keypair().copy(),
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store_primary_1.header_store,
        store_primary_1.certificate_store,
        store_primary_1.payload_store,
        /* tx_consensus */ tx_new_certificates_1,
        /* rx_consensus */ rx_feedback_1,
        /* external_consensus */
        tx_get_block_commands_1,
        rx_get_block_commands_1,
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_1, consensus_metrics).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback_1,
        &Registry::new(),
    );

    let registry_1 = Registry::new();
    let metrics_1 = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry_1)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry_1)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry_1)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry_1)),
    };

    // Spawn a `Worker` instance for primary 1.
    Worker::spawn(
        name_1,
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store_primary_1.batch_store,
        metrics_1,
    );

    // Spawn the primary 2 - a peer to fetch missing certificates from
    let (tx_new_certificates_2, _) = test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_get_block_commands_2, rx_get_block_commands_2) =
        test_utils::test_get_block_commands!(1);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    Primary::spawn(
        name_2.clone(),
        authority_2.keypair().copy(),
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store_primary_2.header_store,
        store_primary_2.certificate_store,
        store_primary_2.payload_store,
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        tx_get_block_commands_2,
        rx_get_block_commands_2,
        /* external_consensus */
        None,
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback_2,
        &Registry::new(),
    );

    let registry_2 = Registry::new();
    let metrics_2 = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry_2)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry_2)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry_2)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry_2)),
    };

    // Spawn a `Worker` instance for primary 2.
    Worker::spawn(
        name_2,
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters.clone(),
        store_primary_2.batch_store,
        metrics_2,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(parameters.clone());

    let collection_ids = block_ids;

    // Test get collections
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: collection_ids.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.get_collections(request).await.unwrap();
    let actual_result = response.into_inner().result;

    assert_eq!(2, actual_result.len());

    // We expect to get successfully the batches only for the one collection
    assert_eq!(
        2,
        actual_result
            .iter()
            .filter(|&r| matches!(
                r.retrieval_result,
                Some(types::RetrievalResult::Collection(_))
            ))
            .count()
    );

    for result in actual_result {
        match result.retrieval_result.unwrap() {
            RetrievalResult::Collection(collection) => {
                let id: CertificateDigest = collection.id.unwrap().try_into().unwrap();
                let result_transactions: Vec<Transaction> = collection
                    .transactions
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<Transaction>>();

                if let Some(expected_batch) = batches_map.get(&id) {
                    assert_eq!(
                        result_transactions, *expected_batch.0,
                        "Batch payload doesn't match"
                    );
                } else {
                    panic!("Unexpected batch!");
                }
            }
            _ => {
                panic!("Expected to have received a batch response");
            }
        }
    }
}

async fn fixture_certificate(
    authority: &AuthorityFixture,
    committee: &Committee,
    fixture: &CommitteeFixture,
    header_store: Store<HeaderDigest, Header>,
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    batch_store: Store<BatchDigest, SerializedBatchMessage>,
) -> (Certificate, Batch) {
    let batch = fixture_batch_with_transactions(10);
    let worker_id = 0;

    let message = WorkerMessage::Batch(batch.clone());
    let serialized_batch = bincode::serialize(&message).unwrap();
    let batch_digest = batch.digest();

    let mut payload = IndexMap::new();
    payload.insert(batch_digest, worker_id);

    let header = authority
        .header_builder(committee)
        .payload(payload)
        .build(authority.keypair())
        .unwrap();

    let certificate = fixture.certificate(&header);

    // Write the certificate
    certificate_store.write(certificate.clone()).unwrap();

    // Write the header
    header_store.write(header.clone().id, header.clone()).await;

    // Write the batches to payload store
    payload_store
        .write_all(vec![((batch_digest, worker_id), 0)])
        .await
        .expect("couldn't store batches");

    // Add a batch to the workers store
    batch_store.write(batch_digest, serialized_batch).await;

    (certificate, batch)
}

fn connect_to_validator_client(parameters: Parameters) -> ValidatorClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ValidatorClient::new(channel)
}
