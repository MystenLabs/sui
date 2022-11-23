// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::authority_tests::init_state_with_object_id,
    authority_client::{
        AuthorityAPI, LocalAuthorityClient, LocalAuthorityClientFaultConfig,
        NetworkAuthorityClient, NetworkAuthorityClientMetrics,
    },
    safe_client::SafeClientMetrics,
};
use futures::StreamExt;
use std::sync::Arc;
use sui_types::{
    base_types::{dbg_addr, dbg_object_id, ExecutionDigests},
    batch::UpdateItem,
    object::ObjectFormatOptions,
};

use crate::safe_client::SafeClient;
use typed_store::Map;

//This is the most basic example of how to test the server logic
#[tokio::test]
async fn test_simple_request() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();

    let server = AuthorityServer::new_for_test(
        "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
        Arc::new(authority_state),
        consensus_address,
    );

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
    )
    .await
    .unwrap();

    let req = ObjectInfoRequest::latest_object_info_request(
        object_id,
        Some(ObjectFormatOptions::default()),
    );

    client.handle_object_info_request(req).await.unwrap();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_subscription() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();

    // Start the batch server
    let mut server = AuthorityServer::new_for_test(
        "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
        Arc::new(authority_state),
        consensus_address,
    );
    server.min_batch_size = 10;
    server.max_delay = Duration::from_secs(5);

    let db = server.state.db().clone();
    let db2 = server.state.db().clone();
    let db3 = server.state.db().clone();
    let state = server.state.clone();

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
    )
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    let tx_zero = ExecutionDigests::random();
    for _i in 0u64..105 {
        let ticket = state.batch_notifier.ticket().expect("all good");
        db.perpetual_tables
            .executed_sequence
            .insert(&ticket.seq(), &tx_zero)
            .expect("Failed to write.");
        ticket.notify();
    }
    println!("Sent tickets.");

    println!("Started messahe handling.");
    // TEST 1: Get historical data

    let req = BatchInfoRequest {
        start: Some(12),
        length: 22,
    };
    tokio::time::sleep(Duration::from_millis(10)).await;

    let mut resp = client.handle_batch_stream(req).await.unwrap();

    println!("TEST1: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = resp.next().await {
        let item = data.unwrap();
        match item {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.data().next_sequence_number >= 34 {
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
        for i in 105..120 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let ticket = inner_server2.batch_notifier.ticket().expect("all good");
            db2.perpetual_tables
                .executed_sequence
                .insert(&ticket.seq(), &tx_zero)
                .expect("Failed to write.");
            println!("Send item {i}");
            ticket.notify();
        }
    });

    println!("TEST2: Sending realtime.");

    let req = BatchInfoRequest {
        start: Some(101),
        length: 11,
    };

    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut resp = client.handle_batch_stream(req).await.unwrap();

    println!("TEST2: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = resp.next().await {
        let item = data.unwrap();
        match item {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.data().next_sequence_number >= 112 {
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

    // On Linux, this is 20 because the batch forms continuously from 100 to 109,
    // and then from 110 to 119.
    // while On Mac, this is 15 because the batch stops at 105, and then restarts
    // from 106 to 114.
    // TODO: Figure out why.
    assert!(num_transactions == 15 || num_transactions == 20);

    _handle2.await.expect("Finished sending");
    println!("TEST2: Finished.");

    println!("TEST3: Sending from very latest.");
    tokio::time::sleep(Duration::from_secs(5)).await;

    let req = BatchInfoRequest {
        start: None,
        length: 10,
    };

    // Use 17 since it is prime and unlikely to collide with the exact timing
    // of the tick interval (set to 5 seconds.)
    tokio::time::sleep(Duration::from_millis(17)).await;
    let mut resp = client.handle_batch_stream(req).await.unwrap();

    println!("TEST3: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;
    let mut i = 120;
    let inner_server2 = state.clone();

    loop {
        // Send a transaction
        let ticket = inner_server2.batch_notifier.ticket().expect("all good");
        db3.perpetual_tables
            .executed_sequence
            .insert(&ticket.seq(), &tx_zero)
            .expect("Failed to write.");
        println!("Send item {i}");
        i += 1;
        tokio::time::sleep(Duration::from_millis(17)).await;

        // Then we wait to receive
        if let Some(data) = resp.next().await {
            match data.expect("No error expected here") {
                BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                    println!("Batch(next={})", signed_batch.data().next_sequence_number);
                    num_batches += 1;
                    if signed_batch.data().next_sequence_number >= 129 {
                        break;
                    }
                }
                BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest))) => {
                    println!("Received {seq}");
                    num_transactions += 1;
                }
            }
        }
        ticket.notify();
    }

    assert!(num_batches >= 2);
    assert!(num_transactions >= 10);

    state.batch_notifier.close();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_subscription_safe_client() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();

    // Start the batch server
    let state = Arc::new(authority_state);
    let server = Arc::new(AuthorityServer::new_for_test(
        "/ip4/127.0.0.1/tcp/998/http".parse().unwrap(),
        state.clone(),
        consensus_address,
    ));

    let db = server.state.db().clone();
    let db2 = server.state.db().clone();
    let db3 = server.state.db().clone();

    let _master_safe_client = SafeClient::new(
        LocalAuthorityClient {
            state: state.clone(),
            fault_config: LocalAuthorityClientFaultConfig::default(),
        },
        state.committee_store().clone(),
        state.name,
        Arc::new(SafeClientMetrics::new_for_tests()),
    );

    let _join = server
        .spawn_batch_subsystem(10, Duration::from_secs(500))
        .await
        .expect("Problem launching subsystem.");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let tx_zero = ExecutionDigests::random();
    for _i in 0u64..105 {
        let ticket = server.state.batch_notifier.ticket().expect("all good");
        db.perpetual_tables
            .executed_sequence
            .insert(&ticket.seq(), &tx_zero)
            .expect("Failed to write.");
        tokio::time::sleep(Duration::from_millis(10)).await;
        ticket.notify();
    }
    println!("Sent tickets.");

    tokio::task::yield_now().await;

    println!("Started messahe handling.");
    // TEST 1: Get historical data

    let req = BatchInfoRequest {
        start: Some(12),
        length: 22,
    };

    let mut stream1 = _master_safe_client
        .handle_batch_stream(req)
        .await
        .expect("Error following");

    //let bytes: BytesMut = BytesMut::from(&serialize_batch_request(&req)[..]);
    //tx.send(Ok(bytes)).await.expect("Problem sending");

    println!("TEST1: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = stream1.next().await {
        match data.expect("Bad response") {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.data().next_sequence_number >= 34 {
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
    let inner_server2 = server.clone();
    let _handle2 = tokio::spawn(async move {
        for i in 105..120 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let ticket = inner_server2
                .state
                .batch_notifier
                .ticket()
                .expect("all good");
            db2.perpetual_tables
                .executed_sequence
                .insert(&ticket.seq(), &tx_zero)
                .expect("Failed to write.");
            println!("Send item {i}");
            ticket.notify();
        }
    });

    println!("TEST2: Sending realtime.");

    let req = BatchInfoRequest {
        start: Some(101),
        length: 11,
    };

    let mut stream1 = _master_safe_client
        .handle_batch_stream(req)
        .await
        .expect("Error following");

    println!("TEST2: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;

    while let Some(data) = stream1.next().await {
        match &data.expect("No error") {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                println!("Batch(next={})", signed_batch.data().next_sequence_number);
                num_batches += 1;
                if signed_batch.data().next_sequence_number >= 112 {
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

    _handle2.await.expect("Finished sending");
    println!("TEST2: Finished.");

    println!("TEST3: Sending from very latest.");

    let req = BatchInfoRequest {
        start: None,
        length: 10,
    };

    let mut stream1 = _master_safe_client
        .handle_batch_stream(req)
        .await
        .expect("Error following");

    println!("TEST3: Send request.");

    let mut num_batches = 0;
    let mut num_transactions = 0;
    let mut i = 120;
    let inner_server2 = server.clone();

    loop {
        // Send a transaction
        let ticket = inner_server2
            .state
            .batch_notifier
            .ticket()
            .expect("all good");
        db3.perpetual_tables
            .executed_sequence
            .insert(&ticket.seq(), &tx_zero)
            .expect("Failed to write.");
        println!("Send item {i}");
        i += 1;
        tokio::time::sleep(Duration::from_millis(20)).await;
        ticket.notify();
        if i > 129 {
            break;
        }
    }

    // Then we wait to receive
    while let Some(data) = stream1.next().await {
        match data.expect("Bad response") {
            BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                num_batches += 1;
                if signed_batch.data().next_sequence_number >= 129 {
                    break;
                }
            }
            BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest))) => {
                println!("Received {seq}");
                num_transactions += 1;
            }
        }
    }

    assert_eq!(2, num_batches);
    assert_eq!(10, num_transactions);

    server.state.batch_notifier.close();
}
