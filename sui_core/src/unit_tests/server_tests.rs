// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::authority_tests::init_state_with_object_id,
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
};
use std::sync::Arc;
use sui_network::network::NetworkClient;
use sui_types::{
    base_types::{dbg_addr, dbg_object_id, TransactionDigest},
    object::ObjectFormatOptions,
};
use typed_store::Map;

#[tokio::test]
async fn test_start_stop_batch_subsystem() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let mut authority_state = init_state_with_object_id(sender, object_id).await;
    authority_state
        .init_batches_from_database()
        .expect("Init batches failed!");

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "127.0.0.1:0".parse().unwrap();
    let (tx_consensus_listener, _rx_consensus_listener) = tokio::sync::mpsc::channel(1);

    let server = Arc::new(AuthorityServer::new(
        "127.0.0.1".to_string(),
        999,
        65000,
        Arc::new(authority_state),
        consensus_address,
        tx_consensus_listener,
    ));
    let join = server
        .spawn_batch_subsystem(1000, Duration::from_secs(5))
        .await
        .expect("Problem launching subsystem.");

    // Now drop the server to simulate the authority server ending processing.
    server.state.batch_notifier.close();
    drop(server);

    // This should return immediately.
    join.await
        .expect("Error stoping subsystem")
        .expect("Subsystem crashed?");
}

//This is the most basic example of how to test the server logic
#[tokio::test]
async fn test_simple_request() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "127.0.0.1:0".parse().unwrap();
    let (tx_consensus_listener, _rx_consensus_listener) = tokio::sync::mpsc::channel(1);

    let server = AuthorityServer::new(
        "127.0.0.1".to_string(),
        0,
        65000,
        Arc::new(authority_state),
        consensus_address,
        tx_consensus_listener,
    );

    let server_handle = server.spawn().await.unwrap();

    let network_config = NetworkClient::new(
        server_handle.local_addr.ip().to_string(),
        server_handle.local_addr.port(),
        0,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(30),
    );

    let client = NetworkAuthorityClient::new(network_config);

    let req = ObjectInfoRequest::latest_object_info_request(
        object_id,
        Some(ObjectFormatOptions::default()),
    );

    client.handle_object_info_request(req).await.unwrap();
}

#[tokio::test]
async fn test_subscription() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "127.0.0.1:0".parse().unwrap();
    let (tx_consensus_listener, _rx_consensus_listener) = tokio::sync::mpsc::channel(1);

    // Start the batch server
    let mut server = AuthorityServer::new(
        "127.0.0.1".to_string(),
        0,
        65000,
        Arc::new(authority_state),
        consensus_address,
        tx_consensus_listener,
    );
    server.min_batch_size = 10;
    server.max_delay = Duration::from_secs(500);

    let db = server.state.db().clone();
    let db2 = server.state.db().clone();
    let state = server.state.clone();

    let server_handle = server.spawn().await.unwrap();

    let network_config = NetworkClient::new(
        server_handle.local_addr.ip().to_string(),
        server_handle.local_addr.port(),
        0,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(30),
    );

    let client = NetworkAuthorityClient::new(network_config);

    let tx_zero = TransactionDigest::new([0; 32]);
    for _i in 0u64..105 {
        let ticket = state.batch_notifier.ticket().expect("all good");
        db.executed_sequence
            .insert(&ticket.seq(), &tx_zero)
            .expect("Failed to write.");
    }
    println!("Sent tickets.");

    println!("Started messahe handling.");
    // TEST 1: Get historical data

    let req = BatchInfoRequest { start: 12, end: 34 };

    let mut resp = client.handle_batch_stream(req).await.unwrap();

    println!("TEST1: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = resp.next().await {
        let item = data.unwrap();
        match item {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.batch.next_sequence_number >= 34 {
                    break;
                }
            }
            BatchInfoResponseItem(UpdateItem::Transaction((_seq, _digest))) => {
                num_transactions += 1;
            }
        }
    }

    assert_eq!(4, num_batches);
    assert_eq!(30, num_transactions);

    println!("TEST1: Finished.");

    // Test 2: Get subscription data

    // Add data in real time
    let inner_server2 = state.clone();
    let _handle2 = tokio::spawn(async move {
        for i in 105..150 {
            let ticket = inner_server2.batch_notifier.ticket().expect("all good");
            db2.executed_sequence
                .insert(&ticket.seq(), &tx_zero)
                .expect("Failed to write.");
            println!("Send item {i}");
        }
    });

    println!("TEST2: Sending realtime.");

    let req = BatchInfoRequest {
        start: 101,
        end: 112,
    };

    let mut resp = client.handle_batch_stream(req).await.unwrap();

    println!("TEST2: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = resp.next().await {
        let item = data.unwrap();
        match item {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.batch.next_sequence_number >= 112 {
                    break;
                }
            }
            BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest))) => {
                println!("Received {seq}");
                num_transactions += 1;
            }
        }
    }

    assert_eq!(3, num_batches);
    assert_eq!(20, num_transactions);

    println!("TEST2: Finished.");

    state.batch_notifier.close();
}
