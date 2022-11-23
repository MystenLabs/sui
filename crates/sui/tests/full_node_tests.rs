// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Neg;
use std::{collections::BTreeMap, sync::Arc};

use futures::future;
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::rpc_params;
use move_core_types::parser::parse_struct_tag;
use move_core_types::value::MoveStructLayout;
use prometheus::Registry;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
use sui_json_rpc_types::{
    type_and_fields_from_move_struct, EventPage, SuiEvent, SuiEventEnvelope, SuiEventFilter,
    SuiExecuteTransactionResponse, SuiExecutionStatus, SuiMoveStruct, SuiMoveValue,
    SuiTransactionFilter, SuiTransactionResponse,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_node::SuiNode;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::event::BalanceChangeType;
use sui_types::event::Event;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
};
use sui_types::object::{Object, ObjectRead, Owner, PastObjectRead};
use sui_types::query::{EventQuery, TransactionQuery};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    messages::TransactionInfoRequest,
};
use sui_types::{sui_framework_address_concat_string, SUI_FRAMEWORK_OBJECT_ID};
use test_utils::authority::test_and_configure_authority_configs;
use test_utils::messages::make_transactions_with_wallet_context;
use test_utils::messages::{
    get_gas_object_with_wallet_context, make_transfer_object_transaction_with_wallet_context,
};
use test_utils::network::{
    init_cluster_builder_env_aware, start_a_fullnode, start_a_fullnode_with_handle,
};
use test_utils::transaction::{
    create_devnet_nft, delete_devnet_nft, increment_counter,
    publish_basics_package_and_make_counter, transfer_coin,
};
use test_utils::transaction::{wait_for_all_txes, wait_for_tx};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio::time::{sleep, Duration};

#[sim_test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let node = start_a_fullnode(&test_cluster.swarm, false).await?;

    let context = &mut test_cluster.wallet;

    let (transferred_object, _, receiver, digest, _, _) = transfer_coin(context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    // verify that the intermediate sync data is cleared.
    let sync_store = node.state().node_sync_store.clone();
    let epoch_id = 0;
    assert!(sync_store.get_cert(epoch_id, &digest).unwrap().is_none());
    assert!(sync_store.get_effects(epoch_id, &digest).unwrap().is_none());

    // verify that the node has seen the transfer
    let object_read = node.state().get_object_read(&transferred_object).await?;
    let object = object_read.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    // timestamp is recorded
    let ts = node.state().get_timestamp_ms(&digest).await?;
    assert!(ts.is_some());

    assert_eq!(node.state().metrics.num_post_processing_tasks.get(), 1);

    Ok(())
}

#[sim_test]
async fn test_full_node_shared_objects() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let node = start_a_fullnode(&test_cluster.swarm, false).await?;

    let context = &mut test_cluster.wallet;

    let sender = context.config.keystore.addresses().get(0).cloned().unwrap();
    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context, sender).await;

    let (tx_cert, _effects_cert) =
        increment_counter(context, sender, None, package_ref, counter_ref.0).await;
    let digest = tx_cert.transaction_digest;
    wait_for_tx(digest, node.state().clone()).await;

    Ok(())
}

const HOUR_MS: u64 = 3_600_000;

#[tokio::test]
async fn test_full_node_move_function_index() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;
    let sender = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context, sender).await;
    let (tx_cert, _effects_cert) =
        increment_counter(context, sender, None, package_ref, counter_ref.0).await;
    let digest = tx_cert.transaction_digest;

    wait_for_tx(digest, node.state().clone()).await;
    let txes = node.state().get_transactions(
        TransactionQuery::MoveFunction {
            package: package_ref.0,
            module: Some("counter".to_string()),
            function: Some("increment".to_string()),
        },
        None,
        None,
        false,
    )?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node.state().get_transactions(
        TransactionQuery::MoveFunction {
            package: package_ref.0,
            module: None,
            function: None,
        },
        None,
        None,
        false,
    )?;

    // 2 transactions in the package i.e create and increment counter
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    eprint!("start...");
    let txes = node.state().get_transactions(
        TransactionQuery::MoveFunction {
            package: package_ref.0,
            module: Some("counter".to_string()),
            function: None,
        },
        None,
        None,
        false,
    )?;

    // 2 transactions in the package i.e publish and increment
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    Ok(())
}

#[tokio::test]
async fn test_full_node_indexes() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;
    let context = &mut test_cluster.wallet;

    let (transferred_object, sender, receiver, digest, gas, gas_used) =
        transfer_coin(context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    let txes = node.state().get_transactions(
        TransactionQuery::InputObject(transferred_object),
        None,
        None,
        false,
    )?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node.state().get_transactions(
        TransactionQuery::MutatedObject(transferred_object),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes =
        node.state()
            .get_transactions(TransactionQuery::FromAddress(sender), None, None, false)?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes =
        node.state()
            .get_transactions(TransactionQuery::ToAddress(receiver), None, None, false)?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    // Note that this is also considered a tx to the sender, because it mutated
    // one or more of the sender's objects.
    let txes =
        node.state()
            .get_transactions(TransactionQuery::ToAddress(sender), None, None, false)?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    // No transactions have originated from the receiver
    let txes = node.state().get_transactions(
        TransactionQuery::FromAddress(receiver),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 0);

    // timestamp is recorded
    let ts = node.state().get_timestamp_ms(&digest).await?;
    assert!(ts.is_some());

    // This is a poor substitute for the post processing taking some time
    // Unfortunately event store writes seem to add some latency so this wait is needed
    sleep(Duration::from_millis(1000)).await;

    // one event is stored, and can be looked up by digest
    // query by timestamp verifies that a timestamp is inserted, within an hour
    let sender_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "transfer_object".into(),
        sender,
        change_type: BalanceChangeType::Pay,
        owner: Owner::AddressOwner(sender),
        coin_type: "0x2::sui::SUI".to_string(),
        version: SequenceNumber::from_u64(0),
        coin_object_id: transferred_object,
        amount: -100000000000000,
    };
    let recipient_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "transfer_object".into(),
        sender,
        change_type: BalanceChangeType::Receive,
        owner: Owner::AddressOwner(receiver),
        coin_type: "0x2::sui::SUI".to_string(),
        version: SequenceNumber::from_u64(1),
        coin_object_id: transferred_object,
        amount: 100000000000000,
    };
    let gas_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "gas".into(),
        sender,
        change_type: BalanceChangeType::Gas,
        owner: Owner::AddressOwner(sender),
        coin_type: "0x2::sui::SUI".to_string(),
        version: gas.1,
        coin_object_id: gas.0,
        amount: (gas_used as i128).neg(),
    };

    // query all events
    let all_events = node
        .state()
        .get_events(
            EventQuery::TimeRange {
                start_time: ts.unwrap() - HOUR_MS,
                end_time: ts.unwrap() + HOUR_MS,
            },
            None,
            10,
            false,
        )
        .await?;
    assert_eq!(all_events[0].1.tx_digest.unwrap(), digest);
    let all_events = all_events
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(all_events.len(), 3);
    assert_eq!(
        all_events,
        vec![
            gas_event.clone(),
            sender_event.clone(),
            recipient_event.clone()
        ]
    );

    // query by sender
    let events_by_sender = node
        .state()
        .get_events(EventQuery::Sender(sender), None, 10, false)
        .await?;
    assert_eq!(events_by_sender[0].1.tx_digest.unwrap(), digest);
    let events_by_sender = events_by_sender
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_sender.len(), 3);
    assert_eq!(
        events_by_sender,
        vec![
            gas_event.clone(),
            sender_event.clone(),
            recipient_event.clone()
        ]
    );

    // query by tx digest
    let events_by_tx = node
        .state()
        .get_events(EventQuery::Transaction(digest), None, 10, false)
        .await?;
    assert_eq!(events_by_tx[0].1.tx_digest.unwrap(), digest);
    let events_by_tx = events_by_tx
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_tx.len(), 3);
    assert_eq!(
        events_by_tx,
        vec![
            gas_event.clone(),
            sender_event.clone(),
            recipient_event.clone()
        ]
    );

    // query by recipient
    let events_by_recipient = node
        .state()
        .get_events(
            EventQuery::Recipient(Owner::AddressOwner(receiver)),
            None,
            10,
            false,
        )
        .await?;
    assert_eq!(events_by_recipient[0].1.tx_digest.unwrap(), digest);
    let events_by_recipient = events_by_recipient
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_recipient.len(), 1);
    assert_eq!(events_by_recipient, vec![recipient_event.clone()]);

    // query by object
    let events_by_object = node
        .state()
        .get_events(EventQuery::Object(transferred_object), None, 10, false)
        .await?;
    assert_eq!(events_by_object[0].1.tx_digest.unwrap(), digest);
    let events_by_object = events_by_object
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_object.len(), 2);
    assert_eq!(
        events_by_object,
        vec![sender_event.clone(), recipient_event.clone()]
    );

    // query by transaction module
    // Query by module ID
    let events_by_module = node
        .state()
        .get_events(
            EventQuery::MoveModule {
                package: SUI_FRAMEWORK_OBJECT_ID,
                module: "transfer_object".to_string(),
            },
            None,
            10,
            false,
        )
        .await?;
    assert_eq!(events_by_module[0].1.tx_digest.unwrap(), digest);
    let events_by_module = events_by_module
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_module.len(), 2);
    assert_eq!(
        events_by_module,
        vec![sender_event.clone(), recipient_event.clone()]
    );

    Ok(())
}

// Test for syncing a node to an authority that already has many txes.
#[sim_test]
async fn test_full_node_cold_sync() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;

    let context = &mut test_cluster.wallet;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let (_transferred_object, _, _, digest, ..) = transfer_coin(context).await?;

    // Make sure the validators are quiescent before bringing up the node.
    sleep(Duration::from_millis(1000)).await;

    // Start a new fullnode that is not on the write path
    let node = start_a_fullnode(&test_cluster.swarm, false).await.unwrap();

    wait_for_tx(digest, node.state().clone()).await;

    let info = node
        .state()
        .handle_transaction_info_request(TransactionInfoRequest {
            transaction_digest: digest,
        })
        .await?;
    assert!(info.signed_effects.is_some());

    Ok(())
}

#[sim_test]
async fn test_full_node_sync_flood() -> Result<(), anyhow::Error> {
    let test_cluster = init_cluster_builder_env_aware().build().await?;
    let sender = test_cluster.get_address_0();
    let context = test_cluster.wallet;

    // Start a new fullnode that is not on the write path
    let node = start_a_fullnode(&test_cluster.swarm, false).await.unwrap();

    let mut futures = Vec::new();

    let (package_ref, counter_ref) =
        publish_basics_package_and_make_counter(&context, sender).await;

    let context = Arc::new(Mutex::new(context));

    // Start up 5 different tasks that all spam txs at the authorities.
    for _i in 0..5 {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let context = context.clone();
        tokio::task::spawn(async move {
            let (sender, object_to_split, gas_obj) = {
                let context = &mut context.lock().await;

                let sender = context.config.keystore.addresses().get(0).cloned().unwrap();

                let mut coins = context.gas_objects(sender).await.unwrap();
                let object_to_split = coins.swap_remove(0).1.reference.to_object_ref();
                let gas_obj = coins.swap_remove(0).1.reference.to_object_ref();
                (sender, object_to_split, gas_obj)
            };

            let mut owned_tx_digest = None;
            let mut shared_tx_digest = None;
            let gas_object_id = gas_obj.0;
            for _ in 0..10 {
                let res = {
                    let context = &mut context.lock().await;
                    SuiClientCommands::SplitCoin {
                        amounts: Some(vec![1]),
                        count: None,
                        coin_id: object_to_split.0,
                        gas: Some(gas_object_id),
                        gas_budget: 50000,
                    }
                    .execute(context)
                    .await
                    .unwrap()
                };

                owned_tx_digest = if let SuiClientCommandResult::SplitCoin(resp) = res {
                    Some(resp.certificate.transaction_digest)
                } else {
                    panic!("transfer command did not return WalletCommandResult::Transfer");
                };

                let context = &context.lock().await;
                shared_tx_digest = Some(
                    increment_counter(
                        context,
                        sender,
                        Some(gas_object_id),
                        package_ref,
                        counter_ref.0,
                    )
                    .await
                    .0
                    .transaction_digest,
                );
            }
            tx.send((owned_tx_digest.unwrap(), shared_tx_digest.unwrap()))
                .unwrap();
        });
        futures.push(rx);
    }

    // make sure the node syncs up to the last digest sent by each task.
    let digests = future::join_all(futures)
        .await
        .iter()
        .map(|r| r.clone().unwrap())
        .flat_map(|(a, b)| std::iter::once(a).chain(std::iter::once(b)))
        .collect();
    wait_for_all_txes(digests, node.state().clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_full_node_transaction_streaming_basic() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut test_cluster.wallet;

    // Start a new fullnode that is not on the write path
    let fullnode = start_a_fullnode_with_handle(&test_cluster.swarm, None, None, false)
        .await
        .unwrap();
    let ws_client = fullnode.ws_client.as_ref().unwrap();
    let node = fullnode.sui_node;

    let mut sub: Subscription<SuiTransactionResponse> = ws_client
        .subscribe(
            "sui_subscribeTransaction",
            rpc_params![SuiTransactionFilter::Any],
            "sui_unsubscribeTransaction",
        )
        .await
        .unwrap();
    let mut digests = Vec::with_capacity(3);
    for _i in 0..3 {
        let (_, _, _, digest, _, _) = transfer_coin(context).await?;
        digests.push(digest);
    }
    wait_for_all_txes(digests.clone(), node.state().clone()).await;

    // Wait for streaming
    for digest in digests.iter().take(3) {
        match timeout(Duration::from_secs(3), sub.next()).await {
            Ok(Some(Ok(resp))) => {
                assert_eq!(&resp.certificate.transaction_digest, digest);
            }
            other => panic!(
                "Failed to get Ok item from transaction streaming, but {:?}",
                other
            ),
        };
    }

    // No more
    match timeout(Duration::from_secs(3), sub.next()).await {
        Err(_) => (),
        other => panic!(
            "Expect to time out because no new txs are coming in. Got {:?}",
            other
        ),
    }

    // Node Config without websocket_address does not create a transaction streamer
    let full_node = start_a_fullnode_with_handle(&test_cluster.swarm, None, None, true).await?;
    assert!(full_node.sui_node.state().transaction_streamer.is_none());

    Ok(())
}

#[tokio::test]
async fn test_full_node_sub_and_query_move_event_ok() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut test_cluster.wallet;

    // Start a new fullnode that is not on the write path
    let fullnode = start_a_fullnode_with_handle(&test_cluster.swarm, None, None, false)
        .await
        .unwrap();
    let node = fullnode.sui_node;
    let ws_client = fullnode.ws_client.as_ref().unwrap();

    let mut sub: Subscription<SuiEventEnvelope> = ws_client
        .subscribe(
            "sui_subscribeEvent",
            rpc_params![SuiEventFilter::MoveEventType(
                sui_framework_address_concat_string("::devnet_nft::MintNFTEvent")
            )],
            "sui_unsubscribeEvent",
        )
        .await
        .unwrap();

    let (sender, object_id, digest) = create_devnet_nft(context).await?;
    wait_for_tx(digest, node.state().clone()).await;

    let struct_tag_str = sui_framework_address_concat_string("::devnet_nft::MintNFTEvent");

    // Wait for streaming
    let bcs = match timeout(Duration::from_secs(5), sub.next()).await {
        Ok(Some(Ok(SuiEventEnvelope {
            event: SuiEvent::MoveEvent {
                type_, fields, bcs, ..
            },
            ..
        }))) => {
            assert_eq!(type_, struct_tag_str,);
            assert_eq!(
                fields,
                Some(SuiMoveStruct::WithFields(BTreeMap::from([
                    ("creator".into(), SuiMoveValue::Address(sender)),
                    (
                        "name".into(),
                        SuiMoveValue::String("example_nft_name".into())
                    ),
                    (
                        "object_id".into(),
                        SuiMoveValue::Address(SuiAddress::from(object_id))
                    ),
                ])))
            );
            bcs
        }
        other => panic!("Failed to get SuiEvent, but {:?}", other),
    };

    let type_ = sui_framework_address_concat_string("::devnet_nft::MintNFTEvent");
    let type_tag = parse_struct_tag(&type_).unwrap();
    let expected_parsed_event =
        Event::move_event_to_move_struct(&type_tag, &bcs, &*node.state().module_cache).unwrap();
    let (_, expected_parsed_event) =
        type_and_fields_from_move_struct(&type_tag, expected_parsed_event);
    let expected_event = SuiEvent::MoveEvent {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "devnet_nft".into(),
        sender,
        type_,
        fields: Some(expected_parsed_event),
        bcs,
    };

    // Query by move event struct name
    let events_by_sender = node
        .state()
        .get_events(EventQuery::MoveEvent(struct_tag_str), None, 10, false)
        .await?;
    assert_eq!(events_by_sender.len(), 1);
    assert_eq!(events_by_sender[0].1.event, expected_event);
    assert_eq!(events_by_sender[0].1.tx_digest.unwrap(), digest);

    // No more
    match timeout(Duration::from_secs(5), sub.next()).await {
        Err(_) => (),
        other => panic!(
            "Expect to time out because no new events are coming in. Got {:?}",
            other
        ),
    }

    Ok(())
}

// Test fullnode has event read jsonrpc endpoints working
#[tokio::test]
async fn test_full_node_event_read_api_ok() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let sender = test_cluster.get_address_0();
    let receiver = test_cluster.get_address_1();
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;
    let jsonrpc_client = &test_cluster.fullnode_handle.as_ref().unwrap().rpc_client;

    let (transferred_object, _, _, digest, gas, gas_used) = transfer_coin(context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    let txes = node.state().get_transactions(
        TransactionQuery::InputObject(transferred_object),
        None,
        None,
        false,
    )?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    // timestamp is recorded
    let ts = node.state().get_timestamp_ms(&digest).await?;
    assert!(ts.is_some());

    // This is a poor substitute for the post processing taking some time
    sleep(Duration::from_millis(1000)).await;
    let sender_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "transfer_object".into(),
        sender,
        change_type: BalanceChangeType::Pay,
        owner: Owner::AddressOwner(sender),
        coin_type: "0x2::sui::SUI".to_string(),
        version: SequenceNumber::from_u64(0),
        coin_object_id: transferred_object,
        amount: -100000000000000,
    };
    let recipient_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "transfer_object".into(),
        sender,
        change_type: BalanceChangeType::Receive,
        owner: Owner::AddressOwner(receiver),
        coin_type: "0x2::sui::SUI".to_string(),
        version: SequenceNumber::from_u64(1),
        coin_object_id: transferred_object,
        amount: 100000000000000,
    };
    let gas_event = SuiEvent::CoinBalanceChange {
        package_id: ObjectID::from_hex_literal("0x2").unwrap(),
        transaction_module: "gas".into(),
        sender,
        change_type: BalanceChangeType::Gas,
        owner: Owner::AddressOwner(sender),
        coin_type: "0x2::sui::SUI".to_string(),
        version: gas.1,
        coin_object_id: gas.0,
        amount: (gas_used as i128).neg(),
    };

    // query by sender
    let params = rpc_params![EventQuery::Sender(sender), None::<u64>, 10, false];

    let events_by_sender: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_sender.data[0].tx_digest.unwrap(), digest);
    let events_by_sender = events_by_sender
        .data
        .into_iter()
        .map(|envelope| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_sender.len(), 3);
    assert_eq!(
        events_by_sender,
        vec![
            gas_event.clone(),
            sender_event.clone(),
            recipient_event.clone()
        ]
    );

    // query by tx digest
    let params = rpc_params![EventQuery::Transaction(digest), None::<u64>, 10, false];
    let events_by_tx: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_tx.data[0].tx_digest.unwrap(), digest);
    let events_by_tx = events_by_tx
        .data
        .into_iter()
        .map(|envelope| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_tx.len(), 3);
    assert_eq!(
        events_by_tx,
        vec![
            gas_event.clone(),
            sender_event.clone(),
            recipient_event.clone()
        ]
    );

    // query by recipient
    let params = rpc_params![
        EventQuery::Recipient(Owner::AddressOwner(receiver)),
        None::<u64>,
        10,
        false
    ];
    let events_by_recipient: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_recipient.data[0].tx_digest.unwrap(), digest);
    let events_by_recipient = events_by_recipient
        .data
        .into_iter()
        .map(|envelope| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_recipient.len(), 1);
    assert_eq!(events_by_recipient, vec![recipient_event.clone()]);

    // query by object
    let params = rpc_params![
        EventQuery::Object(transferred_object),
        None::<u64>,
        10,
        false
    ];
    let events_by_object: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_object.data[0].tx_digest.unwrap(), digest);
    let events_by_object = events_by_object
        .data
        .into_iter()
        .map(|envelope| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_object.len(), 2);
    assert_eq!(
        events_by_object,
        vec![sender_event.clone(), recipient_event.clone()]
    );

    // query by transaction module
    let params = rpc_params![
        EventQuery::MoveModule {
            package: SUI_FRAMEWORK_OBJECT_ID,
            module: "transfer_object".to_string()
        },
        None::<u64>,
        10,
        false
    ];
    let events_by_module: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_module.data[0].tx_digest.unwrap(), digest);
    let events_by_module = events_by_module
        .data
        .into_iter()
        .map(|envelope| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_module.len(), 2);
    assert_eq!(
        events_by_module,
        vec![sender_event.clone(), recipient_event.clone()]
    );

    let (_sender, _object_id, digest2) = create_devnet_nft(context).await?;
    wait_for_tx(digest2, node.state().clone()).await;

    let struct_tag_str = sui_framework_address_concat_string("::devnet_nft::MintNFTEvent");
    let ts2 = node.state().get_timestamp_ms(&digest2).await?;

    // query by move event struct name
    let params = rpc_params![
        EventQuery::MoveEvent(struct_tag_str),
        None::<u64>,
        10,
        false
    ];
    let events_by_sender: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events_by_sender.data.len(), 1);
    assert_eq!(events_by_sender.data[0].tx_digest.unwrap(), digest2);

    // query all transactions

    let params = rpc_params![
        EventQuery::TimeRange {
            start_time: ts.unwrap() - HOUR_MS,
            end_time: ts2.unwrap() + HOUR_MS
        },
        None::<u64>,
        10,
        false
    ];
    let all_events: EventPage = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    // The first txn emits TransferObject(sender), TransferObject(recipient), Gas
    // The second txn emits MoveEvent, NewObject and Gas
    assert_eq!(all_events.data.len(), 6);
    let tx_digests: Vec<TransactionDigest> = all_events
        .data
        .iter()
        .map(|envelope| envelope.tx_digest.unwrap())
        .collect();
    // Sorted in ascending time
    assert_eq!(
        tx_digests,
        vec![digest, digest, digest, digest2, digest2, digest2]
    );

    Ok(())
}

#[sim_test]
async fn test_full_node_transaction_orchestrator_basic() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let node = start_a_fullnode(&test_cluster.swarm, false).await?;

    let context = &mut test_cluster.wallet;
    let transaction_orchestrator = node
        .transaction_orchestrator()
        .expect("Fullnode should have transaction orchestrator toggled on.");
    let mut rx = node
        .subscribe_to_transaction_orchestrator_effects()
        .expect("Fullnode should have transaction orchestrator toggled on.");

    let txn_count = 4;
    let mut txns = make_transactions_with_wallet_context(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // Test WaitForLocalExecution
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::WaitForLocalExecution,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    match res {
        ExecuteTransactionResponse::EffectsCert(res) => {
            let (certified_txn, certified_txn_effects) = rx.recv().await.unwrap();
            let (ct, cte, is_executed_locally) = *res;
            assert_eq!(*ct.digest(), digest);
            assert_eq!(*certified_txn.digest(), digest);
            assert_eq!(*cte.digest(), *certified_txn_effects.digest());
            assert!(is_executed_locally);
            // verify that the node has sequenced and executed the txn
            node.state().get_transaction(digest).await
                .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForLocalExecution: {:?}", digest, e));
        }
        other => {
            panic!(
                "WaitForLocalExecution should get EffectCerts, but got: {:?}",
                other
            );
        }
    };

    // Test WaitForEffectsCert
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    match res {
        ExecuteTransactionResponse::EffectsCert(res) => {
            let (certified_txn, certified_txn_effects) = rx.recv().await.unwrap();
            let (ct, cte, is_executed_locally) = *res;
            assert_eq!(*ct.digest(), digest);
            assert_eq!(*certified_txn.digest(), digest);
            assert_eq!(*cte.digest(), *certified_txn_effects.digest());
            assert!(!is_executed_locally);
            wait_for_tx(digest, node.state().clone()).await;
            node.state().get_transaction(digest).await
                .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForEffectsCert: {:?}", digest, e));
        }
        other => {
            panic!(
                "WaitForEffectsCert should get EffectCerts, but got: {:?}",
                other
            );
        }
    };

    // Test WaitForTxCert
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::WaitForTxCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    match res {
        ExecuteTransactionResponse::TxCert(res) => {
            let (certified_txn, _certified_txn_effects) = rx.recv().await.unwrap();
            let ct = *res;
            assert_eq!(*ct.digest(), digest);
            assert_eq!(*certified_txn.digest(), digest);
            wait_for_tx(digest, node.state().clone()).await;
            node.state().get_transaction(digest).await
                .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForTxCert: {:?}", digest, e));
        }
        other => {
            panic!("WaitForTxCert should get TxCert, but got: {:?}", other);
        }
    };

    // Test ImmediateReturn
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::ImmediateReturn,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    match res {
        ExecuteTransactionResponse::ImmediateReturn => {
            let (certified_txn, _certified_txn_effects) = rx.recv().await.unwrap();
            assert_eq!(*certified_txn.digest(), digest);

            wait_for_tx(digest, node.state().clone()).await;
            node.state().get_transaction(digest).await
                .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with ImmediateReturn: {:?}", digest, e));
        }
        other => {
            panic!(
                "ImmediateReturn should get ImmediateReturn, but got: {:?}",
                other
            );
        }
    };

    Ok(())
}

/// Test a validator node does not have transaction orchestrator
#[tokio::test]
async fn test_validator_node_has_no_transaction_orchestrator() {
    let configs = test_and_configure_authority_configs(1);
    let validator_config = &configs.validator_configs()[0];
    let node = SuiNode::start(validator_config, Registry::new())
        .await
        .unwrap();
    assert!(node.transaction_orchestrator().is_none());
    assert!(node
        .subscribe_to_transaction_orchestrator_effects()
        .is_err());
}

#[tokio::test]
async fn test_full_node_transaction_orchestrator_rpc_ok() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.as_ref().unwrap().rpc_client;

    let txn_count = 4;
    let mut txns = make_transactions_with_wallet_context(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();

    // Test request with ExecuteTransactionRequestType::WaitForLocalExecution
    let (tx_bytes, flag, signature, pub_key) = txn.to_network_data_for_execution();
    let params = rpc_params![
        tx_bytes,
        flag,
        signature,
        pub_key,
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];
    let response: SuiExecuteTransactionResponse = jsonrpc_client
        .request("sui_executeTransaction", params)
        .await
        .unwrap();

    if let SuiExecuteTransactionResponse::EffectsCert {
        certificate,
        effects: _,
        confirmed_local_execution,
    } = response
    {
        assert_eq!(&certificate.transaction_digest, tx_digest);
        assert!(confirmed_local_execution);
    } else {
        panic!("Expect EffectsCert but got {:?}", response);
    }

    let _response: SuiTransactionResponse = jsonrpc_client
        .request("sui_getTransaction", rpc_params![*tx_digest])
        .await
        .unwrap();

    // Test request with ExecuteTransactionRequestType::WaitForEffectsCert
    let (tx_bytes, flag, signature, pub_key) = txn.to_network_data_for_execution();
    let params = rpc_params![
        tx_bytes,
        flag,
        signature,
        pub_key,
        ExecuteTransactionRequestType::WaitForEffectsCert
    ];
    let response: SuiExecuteTransactionResponse = jsonrpc_client
        .request("sui_executeTransaction", params)
        .await
        .unwrap();

    if let SuiExecuteTransactionResponse::EffectsCert {
        certificate,
        effects: _,
        confirmed_local_execution,
    } = response
    {
        assert_eq!(&certificate.transaction_digest, tx_digest);
        assert!(!confirmed_local_execution);
    } else {
        panic!("Expect EffectsCert but got {:?}", response);
    }

    // Test request with ExecuteTransactionRequestType::WaitForTxCert
    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, flag, signature, pub_key) = txn.to_network_data_for_execution();
    let params = rpc_params![
        tx_bytes,
        flag,
        signature,
        pub_key,
        ExecuteTransactionRequestType::WaitForTxCert
    ];
    let response: SuiExecuteTransactionResponse = jsonrpc_client
        .request("sui_executeTransaction", params)
        .await
        .unwrap();

    if let SuiExecuteTransactionResponse::TxCert { certificate } = response {
        assert_eq!(&certificate.transaction_digest, tx_digest);
    } else {
        panic!("Expect TxCert but got {:?}", response);
    }

    // Test request with ExecuteTransactionRequestType::ImmediateReturn
    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, flag, signature, pub_key) = txn.to_network_data_for_execution();
    let params = rpc_params![
        tx_bytes,
        flag,
        signature,
        pub_key,
        ExecuteTransactionRequestType::ImmediateReturn
    ];
    let response: SuiExecuteTransactionResponse = jsonrpc_client
        .request("sui_executeTransaction", params)
        .await
        .unwrap();

    if let SuiExecuteTransactionResponse::ImmediateReturn {
        tx_digest: transaction_digest,
    } = response
    {
        assert_eq!(&transaction_digest, tx_digest);
    } else {
        panic!("Expect ImmediateReturn but got {:?}", response);
    }

    Ok(())
}

async fn get_obj_read_from_node(
    node: &SuiNode,
    object_id: ObjectID,
    seq_num: Option<SequenceNumber>,
) -> Result<(ObjectRef, Object, Option<MoveStructLayout>), anyhow::Error> {
    match seq_num {
        None => {
            let object_read = node.state().get_object_read(&object_id).await?;
            match object_read {
                ObjectRead::Exists(obj_ref, object, layout) => Ok((obj_ref, object, layout)),
                _ => {
                    anyhow::bail!("Can't find object {object_id:?} on fullnode.")
                }
            }
        }
        Some(seq_num) => {
            let object_read = node
                .state()
                .get_past_object_read(&object_id, seq_num)
                .await?;
            match object_read {
                PastObjectRead::VersionFound(obj_ref, object, layout) => {
                    Ok((obj_ref, object, layout))
                }
                _ => {
                    anyhow::bail!(
                        "Can't find object {object_id:?} with seq {seq_num:?} on fullnode."
                    )
                }
            }
        }
    }
}

#[sim_test]
async fn test_get_objects_read() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut test_cluster.wallet;
    let node = start_a_fullnode(&test_cluster.swarm, false).await.unwrap();

    // Create the object
    let (sender, object_id, _) = create_devnet_nft(context).await?;
    sleep(Duration::from_secs(15)).await;

    let recipient = context.config.keystore.addresses().get(1).cloned().unwrap();
    assert_ne!(sender, recipient);

    let (object_ref_v1, object_v1, _) = get_obj_read_from_node(&node, object_id, None).await?;

    // Transfer some SUI to recipient
    transfer_coin(context)
        .await
        .expect("Failed to transfer coins to recipient");
    // Transfer the object from sender to recipient
    let gas_ref = get_gas_object_with_wallet_context(context, &sender)
        .await
        .expect("Expect at least one available gas object");
    let nft_transfer_tx = make_transfer_object_transaction_with_wallet_context(
        object_ref_v1,
        gas_ref,
        context,
        sender,
        recipient,
    );
    context.execute_transaction(nft_transfer_tx).await.unwrap();
    sleep(Duration::from_secs(15)).await;

    let (object_ref_v2, object_v2, _) = get_obj_read_from_node(&node, object_id, None).await?;
    assert_ne!(object_ref_v2, object_ref_v1);

    // Delete the object
    let package_ref = node.state().get_framework_object_ref().await.unwrap();
    let (_tx_cert, effects) =
        delete_devnet_nft(context, &recipient, object_ref_v2, package_ref).await;
    assert_eq!(effects.status, SuiExecutionStatus::Success);
    sleep(Duration::from_secs(15)).await;

    // Now test get_object_read
    let object_ref_v3 = match node.state().get_object_read(&object_id).await? {
        ObjectRead::Deleted(obj_ref) => obj_ref,
        other => anyhow::bail!("Expect object {object_id:?} deleted but got {other:?}."),
    };

    let obj_ref_v3 = match node
        .state()
        .get_past_object_read(&object_id, SequenceNumber::from_u64(3))
        .await?
    {
        PastObjectRead::ObjectDeleted(obj_ref) => obj_ref,
        other => anyhow::bail!("Expect object {object_id:?} deleted but got {other:?}."),
    };
    assert_eq!(object_ref_v3, obj_ref_v3);

    let (obj_ref_v2, obj_v2, _) =
        get_obj_read_from_node(&node, object_id, Some(SequenceNumber::from_u64(2))).await?;
    assert_eq!(object_ref_v2, obj_ref_v2);
    assert_eq!(object_v2, obj_v2);
    assert_eq!(obj_v2.owner, Owner::AddressOwner(recipient));
    let (obj_ref_v1, obj_v1, _) =
        get_obj_read_from_node(&node, object_id, Some(SequenceNumber::from_u64(1))).await?;
    assert_eq!(object_ref_v1, obj_ref_v1);
    assert_eq!(object_v1, obj_v1);
    assert_eq!(obj_v1.owner, Owner::AddressOwner(sender));

    match node
        .state()
        .get_past_object_read(&object_id, SequenceNumber::from_u64(4))
        .await?
    {
        PastObjectRead::VersionTooHigh {
            object_id: obj_id,
            asked_version,
            latest_version,
        } => {
            assert_eq!(obj_id, object_id);
            assert_eq!(asked_version, SequenceNumber::from_u64(4));
            assert_eq!(latest_version, SequenceNumber::from_u64(3));
        }
        other => anyhow::bail!(
            "Expect SequenceNumberTooHigh for object {object_id:?} but got {other:?}."
        ),
    };

    Ok(())
}
