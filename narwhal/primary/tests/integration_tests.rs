// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::Parameters;
use config::WorkerId;
use consensus::dag::Dag;
use crypto::{
    ed25519::{Ed25519KeyPair, Ed25519PublicKey},
    traits::{KeyPair, Signer},
    Hash,
};
use node::NodeStorage;
use primary::{PayloadToken, Primary, CHANNEL_CAPACITY};
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use test_utils::{
    certificate, committee, fixture_batch_with_transactions, fixture_header_builder, keys, temp_dir,
};
use tokio::sync::mpsc::channel;
use tonic::transport::Channel;
use types::{
    BatchDigest, Certificate, CertificateDigest, CertificateDigestProto, CollectionErrorType,
    CollectionRetrievalResult, Empty, GetCollectionsRequest, Header, HeaderDigest,
    RemoveCollectionsRequest, RetrievalResult, ValidatorClient,
};
use worker::{SerializedBatchMessage, Worker, WorkerMessage};

#[tokio::test]
async fn test_get_collections() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair = keys().pop().unwrap();
    let name = keypair.public().clone();
    let signer = keypair;
    let committee = committee();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let worker_id = 0;
    let mut header_ids = Vec::new();
    // Blocks/Collections
    let mut collection_ids = Vec::new();
    let key = keys().pop().unwrap();
    let mut missing_block = CertificateDigest::new([0; 32]);

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch.clone(), worker_id)
            .build(|payload| key.sign(payload));

        let certificate = certificate(&header);
        let block_id = certificate.digest();
        collection_ids.push(block_id);

        // Write the certificate
        store
            .certificate_store
            .write(certificate.digest(), certificate.clone())
            .await;

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
            let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
            let serialized_batch = bincode::serialize(&message).unwrap();
            store
                .batch_store
                .write(batch.digest(), serialized_batch)
                .await;
        } else {
            missing_block = block_id;
        }
    }

    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        signer,
        committee.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */ Some(Arc::new(Dag::new(rx_new_certificates).1)),
    );

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_id,
        committee.clone(),
        parameters.clone(),
        store.batch_store.clone(),
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
        .contains("Attemped fetch of no collections!"));

    // Test get 1 collection
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: vec![collection_ids[0].into()],
    });
    let response = client.get_collections(request).await.unwrap();
    let actual_result = response.into_inner().result;

    assert_eq!(1, actual_result.len());

    assert!(matches!(
        actual_result[0].retrieval_result,
        Some(types::RetrievalResult::Batch(_))
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
            .filter(|&r| matches!(r.retrieval_result, Some(types::RetrievalResult::Batch(_))))
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
    let keypair = keys().pop().unwrap();
    let name = keypair.public().clone();
    let signer = keypair;
    let committee = committee();

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let worker_id = 0;
    let mut header_ids = Vec::new();
    // Blocks/Collections
    let mut collection_ids = Vec::new();
    let key = keys().pop().unwrap();

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch.clone(), worker_id)
            .build(|payload| key.sign(payload));

        let certificate = certificate(&header);
        let block_id = certificate.digest();
        collection_ids.push(block_id);

        // Write the certificate
        store
            .certificate_store
            .write(certificate.digest(), certificate.clone())
            .await;

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
            let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
            let serialized_batch = bincode::serialize(&message).unwrap();
            store
                .batch_store
                .write(batch.digest(), serialized_batch)
                .await;
        }
    }

    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        signer,
        committee.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */ Some(Arc::new(Dag::new(rx_new_certificates).1)),
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
            .await
            .unwrap()
            .is_some(),
        "Certificate should still exist"
    );

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_id,
        committee.clone(),
        parameters.clone(),
        store.batch_store.clone(),
    );

    // Test remove no collections
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![],
    });

    let status = client.remove_collections(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Attemped to remove no collections!"));

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
            .await
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
            .await
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

/// Here we test the ability on our code to synchronize missing certificates
/// by requesting them from other peers. On this example we emulate 2 authorities
/// (2 separate primary nodes) where we store a certificate on each one. Then we
/// are requesting via the get_collections call to the primary 1 to fetch the
/// collections for both certificates. Since primary 1 knows only about the
/// certificate 1 we expect to sync with primary 2 to fetch the unknown
/// certificate 2 after it has been processed for causal completion & validation.
///
/// TODO: the code will not successfully fetch the batches for certificate 2
/// since we don't block on waiting to sync the batches. That will be included
/// in a follow up PR (see https://github.com/MystenLabs/narwhal/issues/223).
/// Then this test will be refactored to test only for successful responses.
#[tokio::test]
async fn test_get_collections_with_missing_certificates() {
    // GIVEN keys for two primary nodes
    let mut k = keys();

    let keypair_1 = k.pop().unwrap();
    let name_1 = keypair_1.public().clone();

    let keypair_2 = k.pop().unwrap();
    let name_2 = keypair_2.public().clone();

    let committee = committee();
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores for the 2 primaries
    let store_primary_1 = NodeStorage::reopen(temp_dir());
    let store_primary_2 = NodeStorage::reopen(temp_dir());

    let worker_id = 0;

    // AND generate and store the certificates
    let signer_1 = keys().remove(0);

    // The certificate_1 will be stored in primary 1
    let certificate_1 = fixture_certificate(
        signer_1,
        store_primary_1.header_store.clone(),
        store_primary_1.certificate_store.clone(),
        store_primary_1.payload_store.clone(),
        store_primary_1.batch_store.clone(),
    )
    .await;

    // pop first to skip
    let signer_2 = keys().remove(1);

    // The certificate_2 will be stored in primary 2
    let certificate_2 = fixture_certificate(
        signer_2,
        store_primary_2.header_store.clone(),
        store_primary_2.certificate_store.clone(),
        store_primary_2.payload_store.clone(),
        store_primary_2.batch_store.clone(),
    )
    .await;

    let block_ids = vec![certificate_1.digest(), certificate_2.digest()];

    // Spawn the primary 1 (which will be the one that we'll interact with)
    let (tx_new_certificates_1, rx_new_certificates_1) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback_1, rx_feedback_1) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name_1.clone(),
        keypair_1,
        committee.clone(),
        parameters.clone(),
        store_primary_1.header_store,
        store_primary_1.certificate_store,
        store_primary_1.payload_store,
        /* tx_consensus */ tx_new_certificates_1,
        /* rx_consensus */ rx_feedback_1,
        /* external_consensus */ Some(Arc::new(Dag::new(rx_new_certificates_1).1)),
    );

    // Spawn a `Worker` instance for primary 1.
    Worker::spawn(
        name_1,
        worker_id,
        committee.clone(),
        parameters.clone(),
        store_primary_1.batch_store,
    );

    // Spawn the primary 2 - a peer to fetch missing certificates from
    let (tx_new_certificates_2, rx_new_certificates_2) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback_2, rx_feedback_2) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name_2.clone(),
        keypair_2,
        committee.clone(),
        parameters.clone(),
        store_primary_2.header_store,
        store_primary_2.certificate_store,
        store_primary_2.payload_store,
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        /* external_consensus */ Some(Arc::new(Dag::new(rx_new_certificates_2).1)),
    );

    // Spawn a `Worker` instance for primary 2.
    Worker::spawn(
        name_2,
        worker_id,
        committee.clone(),
        parameters.clone(),
        store_primary_2.batch_store,
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
        1,
        actual_result
            .iter()
            .filter(|&r| matches!(r.retrieval_result, Some(types::RetrievalResult::Batch(_))))
            .count()
    );

    // The second certificate, which was missing from the primary 1, we expect
    // to have fetched it but not be able to fetch its payload. So the expected
    // error would be CollectionError.
    // TODO: once we plugin the block_synchronizer to sync missing payload as well
    // then this error shouldn't be thrown anymore
    // (see https://github.com/MystenLabs/narwhal/issues/223)
    let error: &CollectionRetrievalResult = actual_result
        .iter()
        .find(|&r| matches!(r.retrieval_result, Some(types::RetrievalResult::Error(_))))
        .unwrap();

    match error.retrieval_result.as_ref().unwrap() {
        RetrievalResult::Error(err) => {
            assert_eq!(err.error, CollectionErrorType::CollectionError as i32);
            assert_eq!(
                err.id.as_ref().unwrap(),
                &CertificateDigestProto::from(certificate_2.digest())
            );
        }
        _ => {
            panic!("Error was expected");
        }
    }
}

async fn fixture_certificate(
    key: Ed25519KeyPair,
    header_store: Store<HeaderDigest, Header<Ed25519PublicKey>>,
    certificate_store: Store<CertificateDigest, Certificate<Ed25519PublicKey>>,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    batch_store: Store<BatchDigest, SerializedBatchMessage>,
) -> Certificate<Ed25519PublicKey> {
    let batch = fixture_batch_with_transactions(10);
    let worker_id = 0;

    let builder = types::HeaderBuilder::<Ed25519PublicKey>::default();
    let header = builder
        .author(key.public().clone())
        .round(1)
        .parents(
            Certificate::genesis(&committee())
                .iter()
                .map(|x| x.digest())
                .collect(),
        )
        .with_payload_batch(batch.clone(), worker_id)
        .build(|payload| key.sign(payload));

    let certificate = certificate(&header);

    // Write the certificate
    certificate_store
        .write(certificate.digest(), certificate.clone())
        .await;

    // Write the header
    header_store.write(header.clone().id, header.clone()).await;

    // Write the batches to payload store
    payload_store
        .write_all(vec![((batch.clone().digest(), worker_id), 0)])
        .await
        .expect("couldn't store batches");

    // Add a batch to the workers store
    let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
    let serialized_batch = bincode::serialize(&message).unwrap();
    batch_store.write(batch.digest(), serialized_batch).await;

    certificate
}

fn connect_to_validator_client(parameters: Parameters) -> ValidatorClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ValidatorClient::new(channel)
}
