// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{handler, handler::MockHandler},
    block_waiter::{
        BatchResult, BlockError, BlockErrorKind, BlockResult, GetBlockResponse, GetBlocksResponse,
    },
    BlockCommand, BlockWaiter, PrimaryWorkerMessage,
};
use bincode::deserialize;
use crypto::{ed25519::Ed25519PublicKey, traits::VerifyingKey, Hash};
use mockall::*;
use network::PrimaryToWorkerNetwork;
use std::{collections::HashMap, sync::Arc};
use test_utils::{
    certificate, fixture_batch_with_transactions, fixture_header_builder,
    fixture_header_with_payload, keys, resolve_name_and_committee, PrimaryToWorkerMockServer,
};
use tokio::{
    sync::{
        mpsc::{channel, Sender},
        oneshot, watch,
    },
    task::JoinHandle,
    time::{sleep, timeout, Duration},
};
use types::{
    Batch, BatchDigest, BatchMessage, Certificate, CertificateDigest, ReconfigureNotification,
};

#[tokio::test]
async fn test_successfully_retrieve_block() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = fixture_header_with_payload(2);
    let certificate = certificate(&header);
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, rx_get_block) = oneshot::channel();
    let (tx_batch_messages, rx_batch_messages) = channel(10);

    // AND "mock" the batch responses
    let mut expected_batch_messages = HashMap::new();
    for (batch_id, _) in header.payload {
        expected_batch_messages.insert(
            batch_id,
            BatchMessage {
                id: batch_id,
                transactions: Batch(vec![vec![10u8, 5u8, 2u8], vec![8u8, 2u8, 3u8]]),
            },
        );
    }

    // AND spin up a worker node
    let worker_id = 0;
    let worker_address = committee
        .worker(&name, &worker_id)
        .unwrap()
        .primary_to_worker;

    let handle = worker_listener::<Ed25519PublicKey>(
        worker_address,
        expected_batch_messages.clone(),
        tx_batch_messages,
    );

    // AND mock the response from the block synchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(1)
        .return_const(vec![Ok(certificate.clone())]);

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![certificate.clone()]))
        .times(1)
        .return_const(vec![Ok(certificate)]);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_batch_messages,
        Arc::new(mock_handler),
    );

    // WHEN we send a request to get a block
    tx_commands
        .send(BlockCommand::GetBlock {
            id: block_id,
            sender: tx_get_block,
        })
        .await
        .unwrap();

    // Wait for the worker server to complete before continue.
    // Then we'll be confident that the expected batch responses
    // have been sent (via the tx_batch_messages channel though)
    if timeout(Duration::from_millis(4_000), handle).await.is_err() {
        panic!("worker hasn't received expected batch requests")
    }

    // THEN we should expect to get back the result
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    tokio::select! {
        Ok(result) = rx_get_block => {
            assert!(result.is_ok(), "Expected to receive a successful result, instead got error: {}", result.err().unwrap());

            let block = result.unwrap();

            assert_eq!(block.batches.len(), expected_batch_messages.len());
            assert_eq!(block.id, block_id.clone());

            for (_, batch) in expected_batch_messages {
                assert_eq!(batch.transactions.0.len(), 2);
            }
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

#[tokio::test]
async fn test_successfully_retrieve_multiple_blocks() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    let key = keys(None).pop().unwrap();
    let mut block_ids = Vec::new();
    let mut expected_batch_messages = HashMap::new();
    let worker_id = 0;
    let mut expected_get_block_responses = Vec::new();
    let mut certificates = Vec::new();

    for _ in 0..2 {
        let batch_1 = fixture_batch_with_transactions(10);
        let batch_2 = fixture_batch_with_transactions(10);

        let header = fixture_header_builder()
            .with_payload_batch(batch_1.clone(), worker_id)
            .with_payload_batch(batch_2.clone(), worker_id)
            .build(&key)
            .unwrap();

        let certificate = certificate(&header);
        certificates.push(certificate.clone());

        block_ids.push(certificate.digest());

        expected_batch_messages.insert(
            batch_1.digest(),
            BatchMessage {
                id: batch_1.digest(),
                transactions: batch_1.clone(),
            },
        );

        expected_batch_messages.insert(
            batch_2.digest(),
            BatchMessage {
                id: batch_2.digest(),
                transactions: batch_2.clone(),
            },
        );

        let mut batches = vec![
            BatchMessage {
                id: batch_1.digest(),
                transactions: batch_1.clone(),
            },
            BatchMessage {
                id: batch_2.digest(),
                transactions: batch_2.clone(),
            },
        ];

        // sort the batches to make sure that the response is the expected one.
        batches.sort_by(|a, b| a.id.cmp(&b.id));

        expected_get_block_responses.push(Ok(GetBlockResponse {
            id: certificate.digest(),
            batches,
        }));
    }

    // AND add a missing block as well
    let missing_block_id = CertificateDigest::default();
    expected_get_block_responses.push(Err(BlockError {
        id: missing_block_id,
        error: BlockErrorKind::BlockNotFound,
    }));

    block_ids.push(missing_block_id);

    // AND the expected get blocks response
    let expected_get_blocks_response = GetBlocksResponse {
        blocks: expected_get_block_responses,
    };

    // AND spawn a new blocks waiter
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_blocks, rx_get_blocks) = oneshot::channel();
    let (tx_batch_messages, rx_batch_messages) = channel(10);

    // AND spin up a worker node
    let worker_address = committee
        .worker(&name, &worker_id)
        .unwrap()
        .primary_to_worker;

    let handle = worker_listener::<Ed25519PublicKey>(
        worker_address,
        expected_batch_messages.clone(),
        tx_batch_messages,
    );

    // AND mock the responses from the BlockSynchronizer
    let mut expected_result: Vec<Result<Certificate<Ed25519PublicKey>, handler::Error>> =
        certificates.clone().into_iter().map(Ok).collect();

    expected_result.push(Err(handler::Error::BlockNotFound {
        block_id: missing_block_id,
    }));

    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(block_ids.clone()))
        .times(1)
        .return_const(expected_result.clone());

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(certificates))
        .times(1)
        .return_const(expected_result);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_batch_messages,
        Arc::new(mock_handler),
    );

    // WHEN we send a request to get a block
    tx_commands
        .send(BlockCommand::GetBlocks {
            ids: block_ids.clone(),
            sender: tx_get_blocks,
        })
        .await
        .unwrap();

    // Wait for the worker server to complete before continue.
    // Then we'll be confident that the expected batch responses
    // have been sent (via the tx_batch_messages channel though)
    if timeout(Duration::from_millis(4_000), handle).await.is_err() {
        panic!("worker hasn't received expected batch requests")
    }

    // THEN we should expect to get back the result
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    tokio::select! {
        Ok(result) = rx_get_blocks => {
            assert!(result.is_ok(), "Expected to receive a successful get blocks result, instead got error: {:?}", result.err().unwrap());
            assert_eq!(result.unwrap(), expected_get_blocks_response);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

#[tokio::test]
async fn test_one_pending_request_for_block_at_time() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = fixture_header_with_payload(2);
    let certificate = certificate(&header);
    let block_id = certificate.digest();

    // AND
    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(1);
    let (_, rx_batch_messages) = channel(1);

    let get_mock_sender = || {
        let (tx, _) = oneshot::channel();
        tx
    };

    // AND mock the responses from the BlockSynchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(4)
        .return_const(vec![Ok(certificate.clone())]);

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![certificate.clone()]))
        .times(4)
        .return_const(vec![Ok(certificate)]);

    let mut waiter = BlockWaiter {
        name: name.clone(),
        committee: committee.clone(),
        rx_commands,
        pending_get_block: HashMap::new(),
        worker_network: PrimaryToWorkerNetwork::default(),
        rx_reconfigure,
        rx_batch_receiver: rx_batch_messages,
        tx_pending_batch: HashMap::new(),
        tx_get_block_map: HashMap::new(),
        tx_get_blocks_map: HashMap::new(),
        block_synchronizer_handler: Arc::new(mock_handler),
    };

    // WHEN we send GetBlock command
    let result_some = waiter
        .handle_get_block_command(block_id, get_mock_sender())
        .await;

    // AND we send more GetBlock commands
    let mut results_none = Vec::new();
    for _ in 0..3 {
        results_none.push(
            waiter
                .handle_get_block_command(block_id, get_mock_sender())
                .await,
        );
    }

    // THEN
    assert!(
        result_some.is_some(),
        "Expected to have a future to do some further work"
    );

    for result in results_none {
        assert!(
            result.is_none(),
            "Expected to not get a future for further work"
        );
    }
}

#[tokio::test]
async fn test_unlocking_pending_get_block_request_after_response() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = fixture_header_with_payload(2);
    let certificate = certificate(&header);
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (_, rx_commands) = channel(1);
    let (_, rx_batch_messages) = channel(1);

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(3)
        .return_const(vec![Ok(certificate.clone())]);

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![certificate.clone()]))
        .times(3)
        .return_const(vec![Ok(certificate)]);

    let mut waiter = BlockWaiter {
        name: name.clone(),
        committee: committee.clone(),
        rx_commands,
        pending_get_block: HashMap::new(),
        worker_network: PrimaryToWorkerNetwork::default(),
        rx_reconfigure,
        rx_batch_receiver: rx_batch_messages,
        tx_pending_batch: HashMap::new(),
        tx_get_block_map: HashMap::new(),
        tx_get_blocks_map: HashMap::new(),
        block_synchronizer_handler: Arc::new(mock_handler),
    };

    let get_mock_sender = || {
        let (tx, _) = oneshot::channel();
        tx
    };

    // AND we send GetBlock commands
    for _ in 0..3 {
        waiter
            .handle_get_block_command(block_id, get_mock_sender())
            .await;
    }

    // WHEN
    let result = BlockResult::Ok(GetBlockResponse {
        id: block_id,
        batches: vec![],
    });

    waiter.handle_batch_waiting_result(result).await;

    // THEN
    assert!(!waiter.pending_get_block.contains_key(&block_id));
    assert!(!waiter.tx_get_block_map.contains_key(&block_id));
}

#[tokio::test]
async fn test_batch_timeout() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = fixture_header_with_payload(2);
    let certificate = certificate(&header);
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, rx_get_block) = oneshot::channel();
    let (_, rx_batch_messages) = channel(10);

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(1)
        .return_const(vec![Ok(certificate.clone())]);

    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![certificate.clone()]))
        .times(1)
        .return_const(vec![Ok(certificate)]);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_batch_messages,
        Arc::new(mock_handler),
    );

    // WHEN we send a request to get a block
    tx_commands
        .send(BlockCommand::GetBlock {
            id: block_id,
            sender: tx_get_block,
        })
        .await
        .unwrap();

    // THEN we should expect to get back the result
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    tokio::select! {
        Ok(result) = rx_get_block => {
            assert!(result.is_err(), "Expected to receive an error result");

            let block_error = result.err().unwrap();

            assert_eq!(block_error.id, block_id.clone());
            assert_eq!(block_error.error, BlockErrorKind::BatchTimeout);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

#[tokio::test]
async fn test_return_error_when_certificate_is_missing() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND create a certificate but don't store it
    let certificate = Certificate::<Ed25519PublicKey>::default();
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, rx_get_block) = oneshot::channel();
    let (_, rx_batch_messages) = channel(10);

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(1)
        .return_const(vec![Err(handler::Error::BlockDeliveryTimeout { block_id })]);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_batch_messages,
        Arc::new(mock_handler),
    );

    // WHEN we send a request to get a block
    tx_commands
        .send(BlockCommand::GetBlock {
            id: block_id,
            sender: tx_get_block,
        })
        .await
        .unwrap();

    // THEN we should expect to get back the error
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    tokio::select! {
        Ok(result) = rx_get_block => {
            assert!(result.is_err(), "Expected to receive an error result");

            let block_error = result.err().unwrap();

            assert_eq!(block_error.id, block_id.clone());
            assert_eq!(block_error.error, BlockErrorKind::BlockNotFound);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

#[tokio::test]
async fn test_return_error_when_certificate_is_missing_when_get_blocks() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();

    // AND create a certificate but don't store it
    let certificate = Certificate::<Ed25519PublicKey>::default();
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_blocks, rx_get_blocks) = oneshot::channel();
    let (_, rx_batch_messages) = channel(10);

    // AND mock the responses of the BlockSynchronizer
    let mut mock_handler = MockHandler::<Ed25519PublicKey>::new();
    mock_handler
        .expect_get_and_synchronize_block_headers()
        .with(predicate::eq(vec![block_id]))
        .times(1)
        .return_const(vec![Err(handler::Error::BlockNotFound { block_id })]);

    // AND mock the response when we request to synchronise the payloads for non
    // found certificates
    mock_handler
        .expect_synchronize_block_payloads()
        .with(predicate::eq(vec![]))
        .times(1)
        .return_const(vec![]);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        rx_reconfigure,
        rx_commands,
        rx_batch_messages,
        Arc::new(mock_handler),
    );

    // WHEN we send a request to get a block
    tx_commands
        .send(BlockCommand::GetBlocks {
            ids: vec![block_id],
            sender: tx_get_blocks,
        })
        .await
        .unwrap();

    // THEN we should expect to get back the error
    let timer = sleep(Duration::from_millis(5_000));
    tokio::pin!(timer);

    tokio::select! {
        Ok(result) = rx_get_blocks => {
            let results = result.unwrap();
            let r = results.blocks.get(0).unwrap().to_owned();
            let block_error = r.err().unwrap();

            assert_eq!(block_error.id, block_id.clone());
            assert_eq!(block_error.error, BlockErrorKind::BlockNotFound);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

// worker_listener listens to TCP requests. The worker responds to the
// RequestBatch requests for the provided expected_batches.
pub fn worker_listener<PublicKey: VerifyingKey>(
    address: multiaddr::Multiaddr,
    expected_batches: HashMap<BatchDigest, BatchMessage>,
    tx_batch_messages: Sender<BatchResult>,
) -> JoinHandle<()> {
    let mut recv = PrimaryToWorkerMockServer::spawn(address);
    tokio::spawn(async move {
        let mut counter = 0;
        loop {
            let message = recv
                .recv()
                .await
                .expect("Failed to receive network message");
            match deserialize(&message.payload) {
                Ok(PrimaryWorkerMessage::<PublicKey>::RequestBatch(id)) => {
                    if expected_batches.contains_key(&id) {
                        tx_batch_messages
                            .send(Ok(expected_batches.get(&id).cloned().unwrap()))
                            .await
                            .unwrap();

                        counter += 1;

                        // Once all the expected requests have been received, break the loop
                        // of the server.
                        if counter == expected_batches.len() {
                            break;
                        }
                    }
                }
                _ => panic!("Unexpected request received"),
            };
        }
    })
}
