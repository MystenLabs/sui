// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_waiter::{
        BatchMessage, BlockCommand, BlockErrorType, BlockResult, BlockWaiter, GetBlockResponse,
    },
    common,
    common::{certificate, committee_with_base_port, create_db_stores, keys},
    messages::BatchDigest,
    Batch, Certificate, PrimaryWorkerMessage,
};
use bincode::deserialize;
use config::Committee;
use crypto::{
    ed25519::Ed25519PublicKey,
    traits::{KeyPair, VerifyingKey},
    Hash,
};

use crate::block_waiter::BatchResult;
use futures::StreamExt;
use network::SimpleSender;
use std::{collections::HashMap, net::SocketAddr};
use tokio::{
    net::TcpListener,
    sync::mpsc::{channel, Sender},
    task::JoinHandle,
    time::{sleep, timeout, Duration},
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[tokio::test]
async fn test_successfully_retrieve_block() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();

    // AND the necessary keys
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = common::fixture_header_with_payload(2);
    let certificate = certificate(&header);
    certificate_store
        .write(certificate.digest(), certificate.clone())
        .await;

    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, mut rx_get_block) = channel(1);
    let (tx_batch_messages, rx_batch_messages) = channel(10);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        certificate_store.clone(),
        rx_commands,
        rx_batch_messages,
    );

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
        Some(result) = rx_get_block.recv() => {
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
async fn test_one_pending_request_for_block_at_time() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();

    // AND the necessary keys
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = common::fixture_header_with_payload(2);
    let certificate = certificate(&header);
    certificate_store
        .write(certificate.digest(), certificate.clone())
        .await;

    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_, rx_commands) = channel(1);
    let (_, rx_batch_messages) = channel(1);

    let mut waiter = BlockWaiter {
        name: name.clone(),
        committee: committee.clone(),
        certificate_store: certificate_store.clone(),
        rx_commands,
        pending_get_block: HashMap::new(),
        network: SimpleSender::new(),
        rx_batch_receiver: rx_batch_messages,
        tx_pending_batch: HashMap::new(),
        tx_get_block_map: HashMap::new(),
    };

    let get_mock_sender = || {
        let (tx, _) = channel(1);
        tx
    };

    // WHEN we send GetBlock command
    let result_some = waiter
        .handle_command(BlockCommand::GetBlock {
            id: block_id,
            sender: get_mock_sender(),
        })
        .await;

    // AND we send more GetBlock commands
    let mut results_none = Vec::new();
    for _ in 0..3 {
        results_none.push(
            waiter
                .handle_command(BlockCommand::GetBlock {
                    id: block_id,
                    sender: get_mock_sender(),
                })
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
    let (_, certificate_store, _) = create_db_stores();

    // AND the necessary keys
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = common::fixture_header_with_payload(2);
    let certificate = certificate(&header);
    certificate_store
        .write(certificate.digest(), certificate.clone())
        .await;

    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (_, rx_commands) = channel(1);
    let (_, rx_batch_messages) = channel(1);

    let mut waiter = BlockWaiter {
        name: name.clone(),
        committee: committee.clone(),
        certificate_store: certificate_store.clone(),
        rx_commands,
        pending_get_block: HashMap::new(),
        network: SimpleSender::new(),
        rx_batch_receiver: rx_batch_messages,
        tx_pending_batch: HashMap::new(),
        tx_get_block_map: HashMap::new(),
    };

    let get_mock_sender = || {
        let (tx, _) = channel(1);
        tx
    };

    // AND we send GetBlock commands
    for _ in 0..3 {
        waiter
            .handle_command(BlockCommand::GetBlock {
                id: block_id,
                sender: get_mock_sender(),
            })
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
    let (_, certificate_store, _) = create_db_stores();

    // AND the necessary keys
    let (name, committee) = resolve_name_and_committee();

    // AND store certificate
    let header = common::fixture_header_with_payload(2);
    let certificate = certificate(&header);
    certificate_store
        .write(certificate.digest(), certificate.clone())
        .await;

    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, mut rx_get_block) = channel(1);
    let (_, rx_batch_messages) = channel(10);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        certificate_store.clone(),
        rx_commands,
        rx_batch_messages,
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
        Some(result) = rx_get_block.recv() => {
            assert!(result.is_err(), "Expected to receive an error result");

            let block_error = result.err().unwrap();

            assert_eq!(block_error.id, block_id.clone());
            assert_eq!(block_error.error, BlockErrorType::BatchTimeout);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

#[tokio::test]
async fn test_return_error_when_certificate_is_missing() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (name, committee) = resolve_name_and_committee();

    // AND create a certificate but don't store it
    let certificate = Certificate::<Ed25519PublicKey>::default();
    let block_id = certificate.digest();

    // AND spawn a new blocks waiter
    let (tx_commands, rx_commands) = channel(1);
    let (tx_get_block, mut rx_get_block) = channel(1);
    let (_, rx_batch_messages) = channel(10);

    BlockWaiter::spawn(
        name.clone(),
        committee.clone(),
        certificate_store.clone(),
        rx_commands,
        rx_batch_messages,
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
        Some(result) = rx_get_block.recv() => {
            assert!(result.is_err(), "Expected to receive an error result");

            let block_error = result.err().unwrap();

            assert_eq!(block_error.id, block_id.clone());
            assert_eq!(block_error.error, BlockErrorType::BlockNotFound);
        },
        () = &mut timer => {
            panic!("Timeout, no result has been received in time")
        }
    }
}

// helper method to get a name and a committee
fn resolve_name_and_committee() -> (Ed25519PublicKey, Committee<Ed25519PublicKey>) {
    let mut keys = keys();
    let _ = keys.pop().unwrap(); // Skip the header' author.
    let kp = keys.pop().unwrap();
    let name = kp.public().clone();
    let committee = committee_with_base_port(13_000);

    (name, committee)
}

// worker_listener listens to TCP requests. The worker responds to the
// RequestBatch requests for the provided expected_batches.
pub fn worker_listener<PublicKey: VerifyingKey>(
    address: SocketAddr,
    expected_batches: HashMap<BatchDigest, BatchMessage>,
    tx_batch_messages: Sender<BatchResult>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let transport = Framed::new(socket, LengthDelimitedCodec::new());

        println!("Start listening server");

        let (_, mut reader) = transport.split();
        let mut counter = 0;
        loop {
            match reader.next().await {
                Some(Ok(received)) => {
                    let message = received.freeze();
                    match deserialize(&message) {
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
                _ => panic!("Failed to receive network message"),
            }
        }
    })
}
