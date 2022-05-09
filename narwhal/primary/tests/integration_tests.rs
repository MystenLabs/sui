// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::Parameters;
use crypto::{
    ed25519::Ed25519PublicKey,
    traits::{KeyPair, Signer},
    Hash,
};
use node::NodeStorage;
use primary::{Primary, CHANNEL_CAPACITY};
use std::time::Duration;
use test_utils::{
    certificate, committee_with_base_port, fixture_batch_with_transactions, fixture_header_builder,
    keys, temp_dir,
};
use tokio::sync::mpsc::channel;
use types::{
    CertificateDigest, CertificateDigestProto, CollectionRetrievalResult, GetCollectionsRequest,
    ValidatorClient,
};
use worker::{Worker, WorkerMessage};

#[tokio::test]
async fn test_get_collections() {
    let keypair = keys().pop().unwrap();
    let name = keypair.public().clone();
    let signer = keypair;
    let committee = committee_with_base_port(11_000);
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let worker_id = 0;
    // Headers/Collections
    let mut header_ids = Vec::new();
    // Blocks/Certficates
    let mut block_ids = Vec::new();
    let key = keys().pop().unwrap();
    let mut missing_batch = CertificateDigest::new([0; 32]);

    // Generate headers
    for n in 0..5 {
        let batch = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch.clone(), worker_id)
            .build(|payload| key.sign(payload));

        let certificate = certificate(&header);
        let block_id = certificate.digest();

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

        block_ids.push(block_id);

        if n != 4 {
            // Add a batch to the workers store
            let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
            let serialized_batch = bincode::serialize(&message).unwrap();
            store
                .batch_store
                .write(batch.digest(), serialized_batch)
                .await;
        } else {
            missing_batch = block_id;
        }
    }

    let (tx_new_certificates, _rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        signer,
        committee.clone(),
        parameters.clone(),
        store.header_store,
        store.certificate_store,
        store.payload_store,
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* external_consensus */ true,
    );

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_id,
        committee.clone(),
        parameters.clone(),
        store.batch_store,
    );

    let max_grpc_connect_retries = 3;
    let mut grpc_connect_retry_count = 0;
    let dst = format!("http://{}", parameters.consensus_api_grpc.socket_addr);
    let mut client = ValidatorClient::connect(dst.to_owned()).await;
    while client.is_err() {
        client = ValidatorClient::connect(dst.to_owned()).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        grpc_connect_retry_count += 1;
        assert!(grpc_connect_retry_count < max_grpc_connect_retries);
    }

    // Test gRPC server with client call
    let mut client = client.unwrap();
    let collection_ids = block_ids;

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

    // And check missing batch id is correct
    let actual_missing_batch = match errors[0].retrieval_result.as_ref().unwrap() {
        types::RetrievalResult::Error(e) => e.id.as_ref(),
        _ => panic!("Should never hit this branch."),
    };

    assert_eq!(
        &CertificateDigestProto::from(missing_batch),
        actual_missing_batch.unwrap()
    );
}
