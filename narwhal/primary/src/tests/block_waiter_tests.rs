// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{handler, handler::MockHandler},
    block_waiter::{BlockError, BlockErrorKind, GetBlockResponse, GetBlocksResponse},
    BlockWaiter,
};
use anemo::PeerId;
use crypto::traits::KeyPair as _;
use fastcrypto::hash::Hash;
use mockall::*;
use std::sync::Arc;
use test_utils::{
    fixture_batch_with_transactions, fixture_payload, latest_protocol_version, test_network,
    CommitteeFixture,
};
use types::{
    Batch, BatchAPI, BatchMessage, Certificate, CertificateDigest, Header, HeaderAPI,
    MockWorkerToWorker, RequestBatchResponse, WorkerToWorkerServer,
};

#[tokio::test]
async fn test_successfully_retrieve_block() {
    // GIVEN
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();

    // AND store certificate
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(2, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let certificate = fixture.certificate(&header);
    let digest = certificate.digest();

    let network = test_network(primary.network_keypair(), primary.address());

    // AND spin up a worker node
    let worker_id = 0;
    let worker = primary.worker(worker_id);
    let network_key = worker.keypair();
    let worker_name = network_key.public().clone();
    let worker_address = &worker.info().worker_address;
    let mut mock_server = MockWorkerToWorker::new();

    // Mock the batch responses.
    let expected_block_count = header.payload().len();
    for (batch_digest, _) in header.payload() {
        let batch_digest_clone = *batch_digest;
        mock_server
            .expect_request_batch()
            .withf(move |request| request.body().batch == batch_digest_clone)
            .returning(move |_| {
                Ok(anemo::Response::new(RequestBatchResponse {
                    batch: Some(Batch::new(
                        vec![vec![10u8, 5u8, 2u8], vec![8u8, 2u8, 3u8]],
                        &latest_protocol_version(),
                    )),
                }))
            });
    }
    let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
    let _worker_network = worker.new_network(routes);

    let address = worker_address.to_anemo_address().unwrap();
    let peer_id = PeerId(worker_name.0.to_bytes());
    network
        .connect_with_peer_id(address, peer_id)
        .await
        .unwrap();

    // AND mock the response from the block synchronizer
    let mut mock_handler = MockHandler::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![digest]))
        .times(1)
        .return_const(vec![Ok(certificate.clone())]);

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![certificate.clone()]))
        .times(1)
        .return_const(vec![Ok(certificate)]);

    // WHEN we send a request to get a block
    let block_waiter =
        BlockWaiter::new(id, committee, worker_cache, network, Arc::new(mock_handler));
    let mut response = block_waiter.get_blocks(vec![digest]).await.unwrap();

    // THEN we should expect to get back the correct result
    assert_eq!(1, response.blocks.len());
    let block = response.blocks.remove(0).unwrap();
    assert_eq!(block.batches.len(), expected_block_count);
    assert_eq!(block.digest, digest.clone());
    for batch in block.batches {
        assert_eq!(batch.batch.transactions().len(), 2);
    }
}

#[tokio::test]
async fn test_successfully_retrieve_multiple_blocks() {
    // GIVEN
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();

    let mut digests = Vec::new();
    let mut mock_server = MockWorkerToWorker::new();
    let worker_id = 0;
    let mut expected_get_block_responses = Vec::new();
    let mut certificates = Vec::new();

    // Batches to be used as "commons" between headers
    // Practically we want to test the case where different headers happen
    // to refer to batches with same id.
    let common_batch_1 = fixture_batch_with_transactions(10, &latest_protocol_version());
    let common_batch_2 = fixture_batch_with_transactions(10, &latest_protocol_version());

    for i in 0..10 {
        let mut builder = author.header_builder(&committee);

        let batch_1 = fixture_batch_with_transactions(10, &latest_protocol_version());
        let batch_2 = fixture_batch_with_transactions(10, &latest_protocol_version());

        builder = builder
            .with_payload_batch(batch_1.clone(), worker_id, 0)
            .with_payload_batch(batch_2.clone(), worker_id, 0);

        for b in [batch_1.clone(), batch_2.clone()] {
            let digest = b.digest();
            mock_server
                .expect_request_batch()
                .withf(move |request| request.body().batch == digest)
                .returning(move |_| {
                    Ok(anemo::Response::new(RequestBatchResponse {
                        batch: Some(b.clone()),
                    }))
                });
        }

        let mut batches = vec![
            BatchMessage {
                digest: batch_1.digest(),
                batch: batch_1.clone(),
            },
            BatchMessage {
                digest: batch_2.digest(),
                batch: batch_2.clone(),
            },
        ];

        // The first 5 headers will have unique payload.
        // The next 5 will be created with common payload (some similar
        // batches will be used)
        if i > 5 {
            builder = builder
                .with_payload_batch(common_batch_1.clone(), worker_id, 0)
                .with_payload_batch(common_batch_2.clone(), worker_id, 0);

            for b in [common_batch_1.clone(), common_batch_2.clone()] {
                let digest = b.digest();
                mock_server
                    .expect_request_batch()
                    .withf(move |request| request.body().batch == digest)
                    .returning(move |_| {
                        Ok(anemo::Response::new(RequestBatchResponse {
                            batch: Some(b.clone()),
                        }))
                    });
            }

            batches.push(BatchMessage {
                digest: common_batch_1.digest(),
                batch: common_batch_1.clone(),
            });
            batches.push(BatchMessage {
                digest: common_batch_2.digest(),
                batch: common_batch_2.clone(),
            });
        }

        // sort the batches to make sure that the response is the expected one.
        batches.sort_by(|a, b| a.digest.cmp(&b.digest));

        let header = Header::V1(builder.build().unwrap());

        let certificate = fixture.certificate(&header);
        certificates.push(certificate.clone());

        digests.push(certificate.digest());

        expected_get_block_responses.push(Ok(GetBlockResponse {
            digest: certificate.digest(),
            batches,
        }));
    }

    // AND add a missing block as well
    let missing_digest = CertificateDigest::default();
    expected_get_block_responses.push(Err(BlockError {
        digest: missing_digest,
        error: BlockErrorKind::BlockNotFound,
    }));

    digests.push(missing_digest);

    // AND the expected get blocks response
    let expected_get_blocks_response = GetBlocksResponse {
        blocks: expected_get_block_responses,
    };

    let network = test_network(primary.network_keypair(), primary.address());

    // AND spin up a worker node
    let worker = primary.worker(worker_id);
    let network_key = worker.keypair();
    let worker_name = network_key.public().clone();
    let worker_address = &worker.info().worker_address;
    let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
    let _worker_network = worker.new_network(routes);

    let address = worker_address.to_anemo_address().unwrap();
    let peer_id = PeerId(worker_name.0.to_bytes());
    network
        .connect_with_peer_id(address, peer_id)
        .await
        .unwrap();

    // AND mock the responses from the BlockSynchronizer
    let mut expected_result: Vec<Result<Certificate, handler::Error>> =
        certificates.clone().into_iter().map(Ok).collect();

    expected_result.push(Err(handler::Error::BlockNotFound {
        digest: missing_digest,
    }));

    let mut mock_handler = MockHandler::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(digests.clone()))
        .times(1)
        .return_const(expected_result.clone());

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(certificates))
        .times(1)
        .return_const(expected_result);

    // WHEN we send a request to get a block
    let block_waiter =
        BlockWaiter::new(id, committee, worker_cache, network, Arc::new(mock_handler));
    let response = block_waiter.get_blocks(digests).await.unwrap();

    // THEN we should expect to get back the correct result
    assert_eq!(response, expected_get_blocks_response);
}

#[tokio::test]
async fn test_return_error_when_certificate_is_missing() {
    // GIVEN
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();

    // AND create a certificate but don't store it
    let certificate = Certificate::default();
    let digest = certificate.digest();

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![digest]))
        .times(1)
        .return_const(vec![Err(handler::Error::BlockDeliveryTimeout { digest })]);
    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![]))
        .times(1)
        .return_const(vec![]);

    let network = test_network(primary.network_keypair(), primary.address());

    // WHEN we send a request to get a block
    let block_waiter =
        BlockWaiter::new(id, committee, worker_cache, network, Arc::new(mock_handler));
    let mut response = block_waiter.get_blocks(vec![digest]).await.unwrap();

    // THEN we should expect to get back the error
    assert_eq!(1, response.blocks.len());
    let block = response.blocks.remove(0);
    assert!(block.is_err());
    let block_error = block.err().unwrap();
    assert_eq!(block_error.digest, digest.clone());
    assert_eq!(block_error.error, BlockErrorKind::BlockNotFound);
}

#[tokio::test]
async fn test_return_error_when_certificate_is_missing_when_get_blocks() {
    // GIVEN
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();

    // AND create a certificate but don't store it
    let certificate = Certificate::default();
    let digest = certificate.digest();

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![digest]))
        .times(1)
        .return_const(vec![Err(handler::Error::BlockNotFound { digest })]);

    // AND mock the response when we request to synchronise the payloads for non
    // found certificates
    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![]))
        .times(1)
        .return_const(vec![]);

    let network = test_network(primary.network_keypair(), primary.address());

    // WHEN we send a request to get a block
    let block_waiter =
        BlockWaiter::new(id, committee, worker_cache, network, Arc::new(mock_handler));
    let response = block_waiter.get_blocks(vec![digest]).await.unwrap();
    let r = response.blocks.get(0).unwrap().to_owned();
    let block_error = r.err().unwrap();

    assert_eq!(block_error.digest, digest.clone());
    assert_eq!(block_error.error, BlockErrorKind::BlockNotFound);
}
