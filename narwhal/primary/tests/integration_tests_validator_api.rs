// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwap;
use config::{BlockSynchronizerParameters, Committee, Parameters, WorkerId};
use consensus::dag::Dag;
use crypto::PublicKey;
use fastcrypto::{hash::Hash, traits::KeyPair as _};
use indexmap::IndexMap;
use narwhal_primary as primary;
use narwhal_primary::NUM_SHUTDOWN_RECEIVERS;
use network::client::NetworkClient;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};
use storage::{CertificateStore, HeaderStore};
use storage::{NodeStorage, PayloadStore};
use store::{rocks::DBMap, Map};
use test_utils::{
    fixture_batch_with_transactions, make_optimal_certificates, make_optimal_signed_certificates,
    temp_dir, AuthorityFixture, CommitteeFixture,
};
use tokio::sync::watch;
use tonic::transport::Channel;
use types::{
    Batch, BatchAPI, BatchDigest, Certificate, CertificateDigest, CertificateDigestProto,
    CollectionRetrievalResult, Empty, GetCollectionsRequest, Header, PreSubscribedBroadcastSender,
    ReadCausalRequest, RemoveCollectionsRequest, RetrievalResult, Transaction, ValidatorClient,
};
use worker::{TrivialTransactionValidator, Worker};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_get_collections() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let author = fixture.authorities().last().unwrap();
    let signer = author.keypair().copy();
    let client = NetworkClient::new_from_keypair(&author.network_keypair());

    let worker_id = 0;
    let worker_keypair = author.worker(worker_id).keypair().copy();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir(), None);

    let mut header_digests = Vec::new();
    // Blocks/Collections
    let mut collection_digests = Vec::new();
    let mut missing_certificate = CertificateDigest::new([0; 32]);

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch.clone(), worker_id, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.digest();
        collection_digests.push(digest);

        // Write the certificate
        store.certificate_store.write(certificate.clone()).unwrap();

        // Write the header
        store.header_store.write(&header).unwrap();

        header_digests.push(header.clone().digest());

        // Write the batches to payload store
        store
            .payload_store
            .write_all(vec![(batch.clone().digest(), worker_id)])
            .expect("couldn't store batches");
        if n != 4 {
            // Add batches to the workers store
            store.batch_store.insert(&batch.digest(), &batch).unwrap();
        } else {
            missing_certificate = digest;
        }
    }

    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    Primary::spawn(
        author.authority().clone(),
        signer.copy(),
        author.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        parameters.clone(),
        client.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store,
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

    let mut tx_shutdown_worker = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance.
    Worker::spawn(
        author.authority().clone(),
        worker_keypair,
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        parameters.clone(),
        TrivialTransactionValidator::default(),
        client,
        store.batch_store.clone(),
        &mut tx_shutdown_worker,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(15)).await;

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
        collection_ids: vec![collection_digests[0].into()],
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
        collection_ids: collection_digests.iter().map(|&c_id| c_id.into()).collect(),
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
        &CertificateDigestProto::from(missing_certificate),
        actual_missing_collection.unwrap()
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[cfg_attr(windows, ignore)]
async fn test_remove_collections() {
    telemetry_subscribers::init_for_testing();

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let author = fixture.authorities().last().unwrap();
    let signer = author.keypair().copy();
    let network_client = NetworkClient::new_from_keypair(&author.network_keypair());

    let worker_id = 0;
    let worker_keypair = author.worker(worker_id).keypair().copy();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir(), None);
    let mut header_digests = Vec::new();
    // Blocks/Collections
    let mut collection_digests = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1);
    // No need to populate genesis in the Dag

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch.clone(), worker_id, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.digest();
        collection_digests.push(digest);

        // Write the certificate
        store.certificate_store.write(certificate.clone()).unwrap();
        dag.insert(certificate.clone()).await.unwrap();

        // Write the header
        store.header_store.write(&header).unwrap();

        header_digests.push(header.clone().digest());

        // Write the batches to payload store
        store
            .payload_store
            .write_all(vec![(batch.clone().digest(), worker_id)])
            .expect("couldn't store batches");
        if n != 4 {
            // Add batches to the workers store
            store.batch_store.insert(&batch.digest(), &batch).unwrap();
        }
    }

    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    Primary::spawn(
        author.authority().clone(),
        signer.copy(),
        author.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        parameters.clone(),
        network_client.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.proposer_store.clone(),
        store.payload_store.clone(),
        store.vote_digest_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(parameters.clone());

    // Test remove 1 collection without spawning worker. Should result in a connection error
    // when trying to connect to the worker to remove batches.
    let block_to_be_removed = collection_digests.remove(0);
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![block_to_be_removed.into()],
    });

    let status = client.remove_collections(request).await.unwrap_err();

    assert!(
        status
            .message()
            .contains("Removal Error: Network has no connection with peer"),
        "Actual: {:?}",
        status
    );
    assert!(
        store
            .certificate_store
            .read(block_to_be_removed)
            .unwrap()
            .is_some(),
        "Certificate should still exist"
    );

    let mut tx_shutdown_worker = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance.
    Worker::spawn(
        author.authority().clone(),
        worker_keypair,
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        parameters.clone(),
        TrivialTransactionValidator::default(),
        network_client,
        store.batch_store.clone(),
        &mut tx_shutdown_worker,
    );

    // Test remove no collections
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![],
    });

    let status = client.remove_collections(request).await.unwrap_err();

    assert!(
        status
            .message()
            .contains("Attempted to remove no collections!"),
        "Actual: {:?}",
        status
    );

    // Wait until worker is ready. Use at most 10 attempts.
    let mut iter = 0;
    loop {
        iter += 1;
        let request = tonic::Request::new(GetCollectionsRequest {
            collection_ids: vec![block_to_be_removed.into()],
        });
        let status = client.get_collections(request).await;
        if status.is_ok() {
            break;
        }
        if iter == 10 {
            panic!("Last failure: {:?}", status);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

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
        collection_ids: collection_digests.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.remove_collections(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(Empty {}, actual_result);

    assert_eq!(
        store
            .certificate_store
            .read_all(collection_digests.clone())
            .unwrap()
            .iter()
            .filter(|c| c.is_some())
            .count(),
        0,
        "Certificate shouldn't exist"
    );

    // Test removing collections again after they have all been removed, no error
    // returned.
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: collection_digests.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.remove_collections(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(Empty {}, actual_result);
}

#[ignore]
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_read_causal_signed_certificates() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir(), None);
    let primary_store_2: NodeStorage = NodeStorage::reopen(temp_dir(), None);

    let client_1 = NetworkClient::new_from_keypair(&authority_1.network_keypair());
    let client_2 = NetworkClient::new_from_keypair(&authority_2.network_keypair());

    let mut collection_digests: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1);

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
        .map(|a| (a.id(), a.keypair().copy()))
        .collect::<Vec<_>>();
    let (certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

    collection_digests.extend(
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
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = authority_1.keypair().copy();

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        authority_1.authority().clone(),
        keypair_1.copy(),
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        client_1,
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.proposer_store.clone(),
        primary_store_1.payload_store.clone(),
        primary_store_1.vote_digest_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    let (tx_new_certificates_2, rx_new_certificates_2) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates_2) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = authority_2.keypair().copy();
    let name_2 = keypair_2.public().clone();

    // Spawn Primary 2
    Primary::spawn(
        authority_2.authority().clone(),
        keypair_2.copy(),
        authority_2.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        client_2,
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.proposer_store,
        primary_store_2.payload_store,
        primary_store_2.vote_digest_store,
        tx_new_certificates_2,
        rx_feedback_2,
        rx_consensus_round_updates_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, tx_shutdown_2.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown_2,
        tx_feedback_2,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(15)).await;

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
        collection_id: Some(collection_digests[1].into()),
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
        collection_id: Some(collection_digests[0].into()),
    });

    let response = client.read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_read_causal_unsigned_certificates() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = authority_1.keypair().copy();

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = authority_2.keypair().copy();
    let network_keypair_2 = authority_2.network_keypair().copy();

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir(), None);
    let primary_store_2: NodeStorage = NodeStorage::reopen(temp_dir(), None);

    let client_1 = NetworkClient::new_from_keypair(&authority_1.network_keypair());
    let client_2 = NetworkClient::new_from_keypair(&authority_2.network_keypair());

    let mut collection_digests: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1);

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
        &committee,
        1..=4,
        &genesis,
        &committee
            .authorities()
            .map(|authority| authority.id())
            .collect::<Vec<AuthorityIdentifier>>(),
    );

    collection_digests.extend(
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
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        authority_1.authority().clone(),
        keypair_1.copy(),
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        client_1,
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.proposer_store.clone(),
        primary_store_1.payload_store.clone(),
        primary_store_1.vote_digest_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    let (tx_new_certificates_2, rx_new_certificates_2) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates_2) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn Primary 2
    Primary::spawn(
        authority_2.authority().clone(),
        keypair_2.copy(),
        network_keypair_2,
        committee.clone(),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        client_2,
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.proposer_store,
        primary_store_2.payload_store,
        primary_store_2.vote_digest_store,
        tx_new_certificates_2,
        rx_feedback_2,
        rx_consensus_round_updates_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, tx_shutdown_2.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown_2,
        tx_feedback_2,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(15)).await;

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
        collection_id: Some(collection_digests[1].into()),
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
        collection_id: Some(collection_digests[0].into()),
    });

    let status = client.read_causal(request).await.unwrap_err();

    assert!(
        status
            .message()
            .contains("Error when trying to synchronize block headers: BlockNotFound"),
        "Saw unexpected status message: {}",
        status.message()
    );
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
///
// TODO: deflake and re-enable this test.
#[tokio::test(flavor = "current_thread", start_paused = true)]
#[ignore = "flaky"]
async fn test_get_collections_with_missing_certificates() {
    telemetry_subscribers::init_for_testing();

    // GIVEN keys for two primary nodes
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    let parameters_1 = Parameters {
        batch_size: 200, // Two transactions.
        block_synchronizer: BlockSynchronizerParameters {
            range_synchronize_timeout: Duration::from_secs(10),
            certificates_synchronize_timeout: Duration::from_secs(1),
            payload_synchronize_timeout: Duration::from_secs(1),
            payload_availability_timeout: Duration::from_secs(1),
            handler_certificate_deliver_timeout: Duration::from_secs(1),
        },
        ..Parameters::default()
    };

    // AND create separate data stores for the 2 primaries
    let store_primary_1 = NodeStorage::reopen(temp_dir(), None);
    let store_primary_2 = NodeStorage::reopen(temp_dir(), None);

    // AND create separate networks for the 2 primaries
    let client_1 = NetworkClient::new_from_keypair(&authority_1.network_keypair());
    let client_2 = NetworkClient::new_from_keypair(&authority_2.network_keypair());

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

    let worker_id = 0;
    let worker_1_keypair = authority_1.worker(worker_id).keypair().copy();
    let worker_2_keypair = authority_2.worker(worker_id).keypair().copy();

    // AND keep a map of batches and payload
    let mut batches_map = HashMap::new();
    batches_map.insert(certificate_1.digest(), batch_1);
    batches_map.insert(certificate_2.digest(), batch_2);

    let digests = vec![certificate_1.digest(), certificate_2.digest()];

    // Spawn the primary 1 (which will be the one that we'll interact with)
    let (tx_new_certificates_1, rx_new_certificates_1) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_1, rx_feedback_1) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    Primary::spawn(
        authority_1.authority().clone(),
        authority_1.keypair().copy(),
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        parameters_1.clone(),
        client_1.clone(),
        store_primary_1.header_store,
        store_primary_1.certificate_store,
        store_primary_1.proposer_store,
        store_primary_1.payload_store,
        store_primary_1.vote_digest_store,
        tx_new_certificates_1,
        rx_feedback_1,
        rx_consensus_round_updates,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_1, tx_shutdown.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback_1,
        None,
    );

    let mut tx_shutdown_worker_1 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance for primary 1.
    Worker::spawn(
        authority_1.authority().clone(),
        worker_1_keypair,
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        parameters_1.clone(),
        TrivialTransactionValidator::default(),
        client_1,
        store_primary_1.batch_store,
        &mut tx_shutdown_worker_1,
    );

    // Spawn the primary 2 - a peer to fetch missing certificates from
    let (tx_new_certificates_2, _) = test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let parameters_2 = Parameters {
        batch_size: 200, // Two transactions.
        block_synchronizer: BlockSynchronizerParameters {
            range_synchronize_timeout: Duration::from_secs(10),
            certificates_synchronize_timeout: Duration::from_secs(1),
            payload_synchronize_timeout: Duration::from_secs(1),
            payload_availability_timeout: Duration::from_secs(1),
            handler_certificate_deliver_timeout: Duration::from_secs(1),
        },
        ..Parameters::default()
    };

    Primary::spawn(
        authority_2.authority().clone(),
        authority_2.keypair().copy(),
        authority_2.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        parameters_2.clone(),
        client_2.clone(),
        store_primary_2.header_store,
        store_primary_2.certificate_store,
        store_primary_2.proposer_store,
        store_primary_2.payload_store,
        store_primary_2.vote_digest_store,
        tx_new_certificates_2,
        rx_feedback_2,
        rx_consensus_round_updates,
        /* external_consensus */
        None,
        NetworkModel::Asynchronous,
        &mut tx_shutdown_2,
        tx_feedback_2,
        None,
    );

    let mut tx_shutdown_worker_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance for primary 2.
    Worker::spawn(
        authority_2.authority().clone(),
        worker_2_keypair,
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        parameters_2.clone(),
        TrivialTransactionValidator::default(),
        client_2,
        store_primary_2.batch_store,
        &mut tx_shutdown_worker_2,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Test gRPC server with client call
    let mut client = connect_to_validator_client(parameters_1.clone());

    let collection_digests = digests;

    // Test get collections
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: collection_digests.iter().map(|&c_id| c_id.into()).collect(),
    });
    let response = client.get_collections(request).await.unwrap();
    let actual_result = response.into_inner().result;

    assert_eq!(
        2,
        actual_result.len(),
        "Unexpected len: {:?}",
        actual_result
    );

    // We expect to get successfully the batches only for the one collection
    assert_eq!(
        2,
        actual_result
            .iter()
            .filter(|&r| matches!(
                r.retrieval_result,
                Some(types::RetrievalResult::Collection(_))
            ))
            .count(),
        "Unexpected: {:?}",
        actual_result,
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
                        result_transactions,
                        *expected_batch.transactions(),
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
    header_store: HeaderStore,
    certificate_store: CertificateStore,
    payload_store: PayloadStore,
    batch_store: DBMap<BatchDigest, Batch>,
) -> (Certificate, Batch) {
    let batch = fixture_batch_with_transactions(10);
    let worker_id = 0;

    let batch_digest = batch.digest();

    let mut payload = IndexMap::new();
    payload.insert(batch_digest, (worker_id, 0));

    let header = Header::V1(
        authority
            .header_builder(committee)
            .payload(payload)
            .build()
            .unwrap(),
    );

    let certificate = fixture.certificate(&header);

    // Write the certificate
    certificate_store.write(certificate.clone()).unwrap();

    // Write the header
    header_store.write(&header).unwrap();

    // Write the batches to payload store
    payload_store
        .write_all(vec![(batch_digest, worker_id)])
        .expect("couldn't store batches");

    // Add a batch to the workers store
    batch_store.insert(&batch_digest, &batch).unwrap();

    (certificate, batch)
}

fn connect_to_validator_client(parameters: Parameters) -> ValidatorClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ValidatorClient::new(channel)
}
