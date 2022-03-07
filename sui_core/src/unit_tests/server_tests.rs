// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::authority::authority_tests::init_state_with_object_id;
use sui_types::base_types::{dbg_addr, dbg_object_id, TransactionDigest};
use sui_types::object::ObjectFormatOptions;
use sui_types::serialize::{deserialize_message, serialize_object_info_request};
use typed_store::Map;

use super::*;

#[tokio::test]
async fn test_start_stop_batch_subsystem() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    let mut server = AuthorityServer::new("127.0.0.1".to_string(), 999, 65000, authority_state);
    let join = server
        .spawn_batch_subsystem(1000, Duration::from_secs(5))
        .await
        .expect("Problem launching subsystem.");

    // Now drop the server to simulate the authority server ending processing.
    drop(server);

    // This should return immediately.
    join.await.expect("Error stoping subsystem");
}

// Some infra to feed the server messages and receive responses.

use bytes::{Bytes, BytesMut};
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::sink::SinkMapErr;
use futures::{Sink, SinkExt};

type SinkSenderErr =
    SinkMapErr<Sender<Bytes>, fn(<Sender<Bytes> as Sink<Bytes>>::Error) -> std::io::Error>;

struct TestChannel {
    reader: Receiver<Result<BytesMut, std::io::Error>>,
    writer: SinkSenderErr,
}

#[allow(clippy::type_complexity)] // appease clippy, in the tests!
impl TestChannel {
    pub fn new() -> (
        TestChannel,
        (Sender<Result<BytesMut, std::io::Error>>, Receiver<Bytes>),
    ) {
        let (outer_tx, inner_rx) = channel(1000);
        let (inner_tx, outer_rx) = channel(1000);

        let test_channel = TestChannel {
            reader: inner_rx,
            writer: inner_tx
                .sink_map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SOme error!")),
        };

        (test_channel, (outer_tx, outer_rx))
    }
}

impl<'a> RwChannel<'a> for TestChannel {
    type R = Receiver<Result<BytesMut, std::io::Error>>;
    type W = SinkSenderErr;

    fn sink(&mut self) -> &mut Self::W {
        &mut self.writer
    }
    fn stream(&mut self) -> &mut Self::R {
        &mut self.reader
    }
}

//This is the most basic example of how to test the server logic

#[tokio::test]
async fn test_channel_infra() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    let server = Arc::new(AuthorityServer::new(
        "127.0.0.1".to_string(),
        999,
        65000,
        authority_state,
    ));

    let (channel, (mut tx, mut rx)) = TestChannel::new();

    let handle = tokio::spawn(async move {
        server.handle_messages(channel).await;
    });

    let req = ObjectInfoRequest::latest_object_info_request(
        object_id,
        Some(ObjectFormatOptions::default()),
    );

    let bytes: BytesMut = BytesMut::from(&serialize_object_info_request(&req)[..]);
    tx.send(Ok(bytes)).await.expect("Problem sending");
    let resp = rx.next().await;
    assert!(!resp.unwrap().is_empty());

    drop(tx);
    handle.await.expect("Problem closing task");
}

#[tokio::test]
async fn test_subscription() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // Start the batch server
    let mut server = AuthorityServer::new("127.0.0.1".to_string(), 998, 65000, authority_state);

    let db = server.state.db().clone();
    let db2 = server.state.db().clone();

    let _join = server
        .spawn_batch_subsystem(10, Duration::from_secs(500))
        .await
        .expect("Problem launching subsystem.");

    let tx_zero = TransactionDigest::new([0; 32]);
    for i in 0u64..105 {
        db.executed_sequence
            .insert(&i, &tx_zero)
            .expect("Failed to write.");

        server
            .state
            .batch_sender()
            .send_item(i, tx_zero)
            .await
            .expect("Send to the channel.");
    }

    let (channel, (mut tx, mut rx)) = TestChannel::new();

    let server = Arc::new(server);

    let inner_server1 = server.clone();
    let handle1 = tokio::spawn(async move {
        inner_server1.handle_messages(channel).await;
    });

    // TEST 1: Get historical data

    let req = BatchInfoRequest { start: 12, end: 34 };

    let bytes: BytesMut = BytesMut::from(&serialize_batch_request(&req)[..]);
    tx.send(Ok(bytes)).await.expect("Problem sending");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = rx.next().await {
        match deserialize_message(&data[..]).expect("Bad response") {
            SerializedMessage::BatchInfoResp(resp) => match *resp {
                BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                    num_batches += 1;
                    if signed_batch.batch.next_sequence_number >= 34 {
                        break;
                    }
                }
                BatchInfoResponseItem(UpdateItem::Transaction((_seq, _digest))) => {
                    num_transactions += 1;
                }
            },
            _ => {
                panic!("Bad response");
            }
        }
    }

    assert_eq!(4, num_batches);
    assert_eq!(30, num_transactions);

    // Test 2: Get subscription data

    // Add data in real time
    let inner_server2 = server.clone();
    let _handle2 = tokio::spawn(async move {
        for i in 105..150 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            db2.executed_sequence
                .insert(&i, &tx_zero)
                .expect("Failed to write.");
            println!("Send item {}", i);
            inner_server2
                .state
                .batch_sender()
                .send_item(i, tx_zero)
                .await
                .expect("Send to the channel.");
        }
    });

    let req = BatchInfoRequest {
        start: 101,
        end: 112,
    };

    let bytes: BytesMut = BytesMut::from(&serialize_batch_request(&req)[..]);
    tx.send(Ok(bytes)).await.expect("Problem sending");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = rx.next().await {
        match deserialize_message(&data[..]).expect("Bad response") {
            SerializedMessage::BatchInfoResp(resp) => match *resp {
                BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                    num_batches += 1;
                    if signed_batch.batch.next_sequence_number >= 112 {
                        break;
                    }
                }
                BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest))) => {
                    println!("Received {}", seq);
                    num_transactions += 1;
                }
            },
            _ => {
                panic!("Bad response");
            }
        }
    }

    assert_eq!(3, num_batches);
    assert_eq!(20, num_transactions);

    drop(tx);
    handle1.await.expect("Problem closing task");
}
