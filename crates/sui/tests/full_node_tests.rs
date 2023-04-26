// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use futures::future;
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::rpc_params;
use move_core_types::ident_str;
use move_core_types::parser::parse_struct_tag;
use move_core_types::value::MoveStructLayout;
use mysten_metrics::RegistryService;
use prometheus::Registry;
use serde_json::json;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use sui_json_rpc_types::EventFilter;
use sui_json_rpc_types::{
    type_and_fields_from_move_struct, SuiEvent, SuiExecutionStatus, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_node::SuiNode;
use sui_tool::restore_from_db_checkpoint;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::crypto::{get_key_pair, SuiKeyPair};
use sui_types::event::{Event, EventID};
use sui_types::message_envelope::Message;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse, GasData,
    QuorumDriverResponse, TransactionData, TransactionKind, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::object::{Object, ObjectRead, Owner, PastObjectRead};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::query::TransactionFilter;
use sui_types::utils::to_sender_signed_transaction_with_multi_signers;
use sui_types::{base_types::ObjectID, messages::TransactionInfoRequest};
use test_utils::authority::test_and_configure_authority_configs;
use test_utils::messages::make_transactions_with_wallet_context;
use test_utils::messages::{
    get_gas_object_with_wallet_context, make_transfer_object_transaction_with_wallet_context,
};
use test_utils::network::{start_fullnode_from_config, TestClusterBuilder};
use test_utils::transaction::{
    create_devnet_nft, delete_devnet_nft, increment_counter,
    publish_basics_package_and_make_counter, publish_nfts_package, transfer_coin,
};
use test_utils::transaction::{wait_for_all_txes, wait_for_tx};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio::time::{sleep, Duration};
use tracing::info;

#[sim_test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let node = test_cluster.start_fullnode().await?.sui_node;

    let context = &mut test_cluster.wallet;

    // TODO: test fails on CI due to flakiness without this. Once https://github.com/MystenLabs/sui/pull/7056 is
    // merged we should be able to root out the flakiness.
    sleep(Duration::from_millis(10)).await;

    let (transferred_object, _, receiver, digest, _, _) = transfer_coin(context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    // A small delay is needed for post processing operations following the transaction to finish.
    sleep(Duration::from_secs(1)).await;

    // verify that the node has seen the transfer
    let object_read = node.state().get_object_read(&transferred_object)?;
    let object = object_read.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    // timestamp is recorded
    let ts = node.state().get_timestamp_ms(&digest).await?;
    assert!(ts.is_some());

    Ok(())
}

#[sim_test]
async fn test_full_node_shared_objects() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let node = test_cluster.start_fullnode().await?.sui_node;

    let context = &mut test_cluster.wallet;

    let sender = context.config.keystore.addresses().get(0).cloned().unwrap();
    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context, sender).await;

    let response = increment_counter(context, sender, None, package_ref.0, counter_ref.0).await;
    let digest = response.digest;
    wait_for_tx(digest, node.state().clone()).await;

    Ok(())
}

#[sim_test]
async fn test_sponsored_transaction() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let rgp = test_cluster.get_reference_gas_price().await;
    let sender = test_cluster.get_address_0();
    let sponsor = test_cluster.get_address_1();
    let another_addr = test_cluster.get_address_2();

    let context = &mut test_cluster.wallet;

    // This makes sender send one coin to sponsor.
    // The sent coin is used as sponsor gas in the following sponsored tx.
    let (sent_coin, sender_, receiver, _, object_ref, _) = transfer_coin(context).await.unwrap();
    assert_eq!(sender, sender_);
    assert_eq!(sponsor, receiver);
    let context: &WalletContext = &test_cluster.wallet;
    let object_ref = context.get_object_ref(object_ref.0).await?;
    let gas_obj = context.get_object_ref(sent_coin).await?;
    info!("updated obj ref: {:?}", object_ref);
    info!("updated gas ref: {:?}", gas_obj);

    // Construct the sponsored transction
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_object(another_addr, object_ref).unwrap();
        builder.finish()
    };
    let kind = TransactionKind::programmable(pt);
    let tx_data = TransactionData::new_with_gas_data(
        kind,
        sender,
        GasData {
            payment: vec![gas_obj],
            owner: sponsor,
            price: rgp,
            budget: rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        },
    );

    let tx = to_sender_signed_transaction_with_multi_signers(
        tx_data,
        vec![
            context.config.keystore.get_key(&sender).unwrap(),
            context.config.keystore.get_key(&sponsor).unwrap(),
        ],
    );

    context.execute_transaction_block(tx).await.unwrap();

    assert_eq!(sponsor, context.get_object_owner(&sent_coin).await.unwrap(),);
    Ok(())
}

#[sim_test]
async fn test_full_node_move_function_index() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let node = &test_cluster.fullnode_handle.sui_node;
    let sender = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context, sender).await;
    let response = increment_counter(context, sender, None, package_ref.0, counter_ref.0).await;
    let digest = response.digest;

    wait_for_tx(digest, node.state().clone()).await;
    let txes = node.state().get_transactions(
        Some(TransactionFilter::MoveFunction {
            package: package_ref.0,
            module: Some("counter".to_string()),
            function: Some("increment".to_string()),
        }),
        None,
        None,
        false,
    )?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node.state().get_transactions(
        Some(TransactionFilter::MoveFunction {
            package: package_ref.0,
            module: None,
            function: None,
        }),
        None,
        None,
        false,
    )?;

    // 2 transactions in the package i.e create and increment counter
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    eprint!("start...");
    let txes = node.state().get_transactions(
        Some(TransactionFilter::MoveFunction {
            package: package_ref.0,
            module: Some("counter".to_string()),
            function: None,
        }),
        None,
        None,
        false,
    )?;

    // 2 transactions in the package i.e publish and increment
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    Ok(())
}

#[sim_test]
async fn test_full_node_indexes() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await?;
    let node = &test_cluster.fullnode_handle.sui_node;
    let context = &mut test_cluster.wallet;

    let (transferred_object, sender, receiver, digest, _, _) = transfer_coin(context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    let txes = node.state().get_transactions(
        Some(TransactionFilter::InputObject(transferred_object)),
        None,
        None,
        false,
    )?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node.state().get_transactions(
        Some(TransactionFilter::ChangedObject(transferred_object)),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    let txes = node.state().get_transactions(
        Some(TransactionFilter::FromAddress(sender)),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node.state().get_transactions(
        Some(TransactionFilter::ToAddress(receiver)),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    // Note that this is also considered a tx to the sender, because it mutated
    // one or more of the sender's objects.
    let txes = node.state().get_transactions(
        Some(TransactionFilter::ToAddress(sender)),
        None,
        None,
        false,
    )?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    // No transactions have originated from the receiver
    let txes = node.state().get_transactions(
        Some(TransactionFilter::FromAddress(receiver)),
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

    /* // one event is stored, and can be looked up by digest
    // query by timestamp verifies that a timestamp is inserted, within an hour
    let sender_balance_change = BalanceChange {
        change_type: BalanceChangeType::Pay,
        owner: sender,
        coin_type: parse_struct_tag("0x2::sui::SUI").unwrap(),
        amount: -100000000000000,
    };
    let recipient_balance_change = BalanceChange {
        change_type: BalanceChangeType::Receive,
        owner: receiver,
        coin_type: parse_struct_tag("0x2::sui::SUI").unwrap(),
        amount: 100000000000000,
    };
    let gas_balance_change = BalanceChange {
        change_type: BalanceChangeType::Gas,
        owner: sender,
        coin_type: parse_struct_tag("0x2::sui::SUI").unwrap(),
        amount: (gas_used as i128).neg(),
    };

    // query all events
    let all_events = node
        .state()
        .get_transaction_events(
            EventQuery::TimeRange {
                start_time: ts.unwrap() - HOUR_MS,
                end_time: ts.unwrap() + HOUR_MS,
            },
            None,
            100,
            false,
        )
        .await?;
    let all_events = &all_events[all_events.len() - 3..];
    assert_eq!(all_events.len(), 3);
    assert_eq!(all_events[0].1.tx_digest, digest);
    let all_events = all_events
        .iter()
        .map(|(_, envelope)| envelope.event.clone())
        .collect::<Vec<_>>();
    assert_eq!(all_events[0], gas_event.clone());
    assert_eq!(all_events[1], sender_event.clone());
    assert_eq!(all_events[2], recipient_event.clone());

    // query by sender
    let events_by_sender = node
        .state()
        .query_events(EventQuery::Sender(sender), None, 10, false)
        .await?;
    assert_eq!(events_by_sender.len(), 3);
    assert_eq!(events_by_sender[0].1.tx_digest, digest);
    let events_by_sender = events_by_sender
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_sender[0], gas_event.clone());
    assert_eq!(events_by_sender[1], sender_event.clone());
    assert_eq!(events_by_sender[2], recipient_event.clone());

    // query by tx digest
    let events_by_tx = node
        .state()
        .query_events(EventQuery::Transaction(digest), None, 10, false)
        .await?;
    assert_eq!(events_by_tx.len(), 3);
    assert_eq!(events_by_tx[0].1.tx_digest, digest);
    let events_by_tx = events_by_tx
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_tx[0], gas_event);
    assert_eq!(events_by_tx[1], sender_event.clone());
    assert_eq!(events_by_tx[2], recipient_event.clone());

    // query by recipient
    let events_by_recipient = node
        .state()
        .query_events(
            EventQuery::Recipient(Owner::AddressOwner(receiver)),
            None,
            100,
            false,
        )
        .await?;
    assert_eq!(events_by_recipient.last().unwrap().1.tx_digest, digest);
    assert_eq!(events_by_recipient.last().unwrap().1.event, recipient_event);

    // query by object
    let mut events_by_object = node
        .state()
        .query_events(EventQuery::Object(transferred_object), None, 100, false)
        .await?;
    let events_by_object = events_by_object.split_off(events_by_object.len() - 2);
    assert_eq!(events_by_object.len(), 2);
    assert_eq!(events_by_object[0].1.tx_digest, digest);
    let events_by_object = events_by_object
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_object[0], sender_event.clone());
    assert_eq!(events_by_object[1], recipient_event.clone());

    // query by transaction module
    // Query by module ID
    let events_by_module = node
        .state()
        .query_events(
            EventQuery::MoveModule {
                package: SuiFramework::ID,
                module: "unused_input_object".to_string(),
            },
            None,
            10,
            false,
        )
        .await?;
    assert_eq!(events_by_module[0].1.tx_digest, digest);
    let events_by_module = events_by_module
        .into_iter()
        .map(|(_, envelope)| envelope.event)
        .collect::<Vec<_>>();
    assert_eq!(events_by_module.len(), 2);
    assert_eq!(events_by_module[0], sender_event);
    assert_eq!(events_by_module[1], recipient_event);*/

    Ok(())
}

// Test for syncing a node to an authority that already has many txes.
#[sim_test]
async fn test_full_node_cold_sync() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;

    let context = &mut test_cluster.wallet;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let (_transferred_object, _, _, digest, ..) = transfer_coin(context).await?;

    // Make sure the validators are quiescent before bringing up the node.
    sleep(Duration::from_millis(1000)).await;

    // Start a new fullnode that is not on the write path
    let node = test_cluster.start_fullnode().await.unwrap().sui_node;

    wait_for_tx(digest, node.state().clone()).await;

    let info = node
        .state()
        .handle_transaction_info_request(TransactionInfoRequest {
            transaction_digest: digest,
        })
        .await?;
    // Check that it has been executed.
    info.status.into_effects_for_testing();

    Ok(())
}

#[sim_test]
async fn test_full_node_sync_flood() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await?;

    // Start a new fullnode that is not on the write path
    let node = test_cluster.start_fullnode().await.unwrap().sui_node;

    let sender = test_cluster.get_address_0();
    let context = test_cluster.wallet;

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
                let object_to_split = coins.swap_remove(0).1.object_ref();
                let gas_obj = coins.swap_remove(0).1.object_ref();
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
                        serialize_output: false,
                    }
                    .execute(context)
                    .await
                    .unwrap()
                };

                owned_tx_digest = if let SuiClientCommandResult::SplitCoin(resp) = res {
                    Some(resp.digest)
                } else {
                    panic!("transfer command did not return WalletCommandResult::Transfer");
                };

                let context = &context.lock().await;
                shared_tx_digest = Some(
                    increment_counter(
                        context,
                        sender,
                        Some(gas_object_id),
                        package_ref.0,
                        counter_ref.0,
                    )
                    .await
                    .digest,
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

#[sim_test]
async fn test_full_node_sub_and_query_move_event_ok() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await?;

    // Start a new fullnode that is not on the write path
    let fullnode = start_fullnode_from_config(
        test_cluster
            .fullnode_config_builder()
            .with_event_store()
            .build()
            .unwrap(),
    )
    .await
    .unwrap();

    let node = fullnode.sui_node;
    let ws_client = fullnode.ws_client;

    let context = &mut test_cluster.wallet;
    let package_id = publish_nfts_package(context).await.0;

    let struct_tag_str = format!("{package_id}::devnet_nft::MintNFTEvent");
    let struct_tag = parse_struct_tag(&struct_tag_str).unwrap();

    let mut sub: Subscription<SuiEvent> = ws_client
        .subscribe(
            "suix_subscribeEvent",
            rpc_params![EventFilter::MoveEventType(struct_tag.clone())],
            "suix_unsubscribeEvent",
        )
        .await
        .unwrap();

    let (sender, object_id, digest) = create_devnet_nft(context, package_id).await?;
    wait_for_tx(digest, node.state().clone()).await;

    // Wait for streaming
    let bcs = match timeout(Duration::from_secs(5), sub.next()).await {
        Ok(Some(Ok(SuiEvent {
            type_,
            parsed_json,
            bcs,
            ..
        }))) => {
            assert_eq!(&type_, &struct_tag);
            assert_eq!(
                parsed_json,
                json!({
                    "creator" : sender,
                    "name": "example_nft_name",
                    "object_id" : object_id,
                })
            );
            bcs
        }
        other => panic!("Failed to get SuiEvent, but {:?}", other),
    };
    let type_tag = parse_struct_tag(&struct_tag_str).unwrap();
    let expected_parsed_event = Event::move_event_to_move_struct(
        &type_tag,
        &bcs,
        &**node.state().epoch_store_for_testing().module_cache(),
    )
    .unwrap();
    let (_, expected_parsed_event) =
        type_and_fields_from_move_struct(&type_tag, expected_parsed_event);
    let expected_event = SuiEvent {
        id: EventID {
            tx_digest: digest,
            event_seq: 0,
        },
        package_id,
        transaction_module: ident_str!("devnet_nft").into(),
        sender,
        type_: type_tag,
        parsed_json: expected_parsed_event.to_json_value(),
        bcs,
        timestamp_ms: None,
    };

    // get tx events
    let events = test_cluster
        .sui_client()
        .event_api()
        .get_events(digest)
        .await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], expected_event);
    assert_eq!(events[0].id.tx_digest, digest);

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
#[sim_test]
async fn test_full_node_event_read_api_ok() {
    let mut test_cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(50000)
        .enable_fullnode_events()
        .build()
        .await
        .unwrap();

    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.sui_node;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let (package_id, gas_id_1, _, _) = publish_nfts_package(context).await;

    let (transferred_object, _, _, digest, _, _) = transfer_coin(context).await.unwrap();

    wait_for_tx(digest, node.state().clone()).await;

    let txes = node
        .state()
        .get_transactions(
            Some(TransactionFilter::InputObject(transferred_object)),
            None,
            None,
            false,
        )
        .unwrap();

    if gas_id_1 == transferred_object {
        assert_eq!(txes.len(), 2);
        assert!(txes[0] == digest || txes[1] == digest);
    } else {
        assert_eq!(txes.len(), 1);
        assert_eq!(txes[0], digest);
    }

    // timestamp is recorded
    let ts = node.state().get_timestamp_ms(&digest).await.unwrap();
    assert!(ts.is_some());

    // This is a poor substitute for the post processing taking some time
    sleep(Duration::from_millis(1000)).await;

    let (_sender, _object_id, digest2) = create_devnet_nft(context, package_id).await.unwrap();
    wait_for_tx(digest2, node.state().clone()).await;

    // Add a delay to ensure event processing is done after transaction commits.
    sleep(Duration::from_secs(5)).await;

    // query by move event struct name
    let params = rpc_params![digest2];
    let events: Vec<SuiEvent> = jsonrpc_client
        .request("sui_getEvents", params)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id.tx_digest, digest2);
}

#[sim_test]
async fn test_full_node_transaction_orchestrator_basic() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let node = test_cluster.start_fullnode().await?.sui_node;

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
        .execute_transaction_block(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::WaitForLocalExecution,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let ExecuteTransactionResponse::EffectsCert(res) = res;
    let (
        tx,
        QuorumDriverResponse {
            effects_cert: certified_txn_effects,
            events: txn_events,
        },
    ) = rx.recv().await.unwrap().unwrap();
    let (cte, events, is_executed_locally) = *res;
    assert_eq!(*tx.digest(), digest);
    assert_eq!(cte.effects.digest(), *certified_txn_effects.digest());
    assert!(is_executed_locally);
    assert_eq!(events.digest(), txn_events.digest());
    // verify that the node has sequenced and executed the txn
    node.state().get_executed_transaction_and_effects(digest).await
        .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForLocalExecution: {:?}", digest, e));

    // Test WaitForEffectsCert
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction_block(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let ExecuteTransactionResponse::EffectsCert(res) = res;
    let (
        tx,
        QuorumDriverResponse {
            effects_cert: certified_txn_effects,
            events: txn_events,
        },
    ) = rx.recv().await.unwrap().unwrap();
    let (cte, events, is_executed_locally) = *res;
    assert_eq!(*tx.digest(), digest);
    assert_eq!(cte.effects.digest(), *certified_txn_effects.digest());
    assert_eq!(txn_events.digest(), events.digest());
    assert!(!is_executed_locally);
    wait_for_tx(digest, node.state().clone()).await;
    node.state().get_executed_transaction_and_effects(digest).await
        .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForEffectsCert: {:?}", digest, e));

    Ok(())
}

/// Test a validator node does not have transaction orchestrator
#[tokio::test]
async fn test_validator_node_has_no_transaction_orchestrator() {
    let configs = test_and_configure_authority_configs(1);
    let validator_config = &configs.validator_configs()[0];
    let registry_service = RegistryService::new(Registry::new());
    let node = SuiNode::start(validator_config, registry_service, None)
        .await
        .unwrap();
    assert!(node.transaction_orchestrator().is_none());
    assert!(node
        .subscribe_to_transaction_orchestrator_effects()
        .is_err());
}

#[sim_test]
async fn test_execute_tx_with_serialized_signature() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    context
        .config
        .keystore
        .add_key(SuiKeyPair::Secp256k1(get_key_pair().1))?;
    context
        .config
        .keystore
        .add_key(SuiKeyPair::Ed25519(get_key_pair().1))?;

    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let txn_count = 4;
    let txns = make_transactions_with_wallet_context(context, txn_count).await;
    for txn in txns {
        let tx_digest = txn.digest();
        let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
        let params = rpc_params![
            tx_bytes,
            signatures,
            SuiTransactionBlockResponseOptions::new(),
            ExecuteTransactionRequestType::WaitForLocalExecution
        ];
        let response: SuiTransactionBlockResponse = jsonrpc_client
            .request("sui_executeTransactionBlock", params)
            .await
            .unwrap();

        let SuiTransactionBlockResponse {
            digest,
            confirmed_local_execution,
            ..
        } = response;
        assert_eq!(digest, *tx_digest);
        assert!(confirmed_local_execution.unwrap());
    }
    Ok(())
}

#[sim_test]
async fn test_full_node_transaction_orchestrator_rpc_ok() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

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
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];
    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params)
        .await
        .unwrap();

    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    let _response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_getTransactionBlock", rpc_params![*tx_digest])
        .await
        .unwrap();

    // Test request with ExecuteTransactionRequestType::WaitForEffectsCert
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new().with_effects(),
        ExecuteTransactionRequestType::WaitForEffectsCert
    ];
    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params)
        .await
        .unwrap();

    let SuiTransactionBlockResponse {
        effects,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(effects.unwrap().transaction_digest(), tx_digest);
    assert!(!confirmed_local_execution.unwrap());

    Ok(())
}

async fn get_obj_read_from_node(
    node: &SuiNode,
    object_id: ObjectID,
) -> Result<(ObjectRef, Object, Option<MoveStructLayout>), anyhow::Error> {
    if let ObjectRead::Exists(obj_ref, object, layout) = node.state().get_object_read(&object_id)? {
        Ok((obj_ref, object, layout))
    } else {
        anyhow::bail!("Can't find object {object_id:?} on fullnode.")
    }
}

async fn get_past_obj_read_from_node(
    node: &SuiNode,
    object_id: ObjectID,
    seq_num: SequenceNumber,
) -> Result<(ObjectRef, Object, Option<MoveStructLayout>), anyhow::Error> {
    if let PastObjectRead::VersionFound(obj_ref, object, layout) =
        node.state().get_past_object_read(&object_id, seq_num)?
    {
        Ok((obj_ref, object, layout))
    } else {
        anyhow::bail!("Can't find object {object_id:?} with seq {seq_num:?} on fullnode.")
    }
}

#[sim_test]
async fn test_get_objects_read() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let rgp = test_cluster.get_reference_gas_price().await;
    let node = test_cluster.fullnode_handle.sui_node.clone();
    let context = &mut test_cluster.wallet;
    let package_id = publish_nfts_package(context).await.0;

    // Create the object
    let (sender, object_id, _) = create_devnet_nft(context, package_id).await?;
    sleep(Duration::from_secs(3)).await;

    let recipient = context.config.keystore.addresses().get(1).cloned().unwrap();
    assert_ne!(sender, recipient);

    let (object_ref_v1, object_v1, _) = get_obj_read_from_node(&node, object_id).await?;

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
        rgp,
    );
    context
        .execute_transaction_block(nft_transfer_tx)
        .await
        .unwrap();
    sleep(Duration::from_secs(1)).await;

    let (object_ref_v2, object_v2, _) = get_obj_read_from_node(&node, object_id).await?;
    assert_ne!(object_ref_v2, object_ref_v1);

    // Transfer some SUI to recipient
    transfer_coin(context)
        .await
        .expect("Failed to transfer coins to recipient");

    // Delete the object
    let response = delete_devnet_nft(context, &recipient, package_id, object_ref_v2).await;
    assert_eq!(
        *response.effects.unwrap().status(),
        SuiExecutionStatus::Success
    );
    sleep(Duration::from_secs(1)).await;

    // Now test get_object_read
    let object_ref_v3 = match node.state().get_object_read(&object_id)? {
        ObjectRead::Deleted(obj_ref) => obj_ref,
        other => anyhow::bail!("Expect object {object_id:?} deleted but got {other:?}."),
    };

    let read_ref_v3 = match node
        .state()
        .get_past_object_read(&object_id, object_ref_v3.1)?
    {
        PastObjectRead::ObjectDeleted(obj_ref) => obj_ref,
        other => anyhow::bail!("Expect object {object_id:?} deleted but got {other:?}."),
    };
    assert_eq!(object_ref_v3, read_ref_v3);

    let (read_ref_v2, read_obj_v2, _) =
        get_past_obj_read_from_node(&node, object_id, object_ref_v2.1).await?;
    assert_eq!(read_ref_v2, object_ref_v2);
    assert_eq!(read_obj_v2, object_v2);
    assert_eq!(read_obj_v2.owner, Owner::AddressOwner(recipient));

    let (read_ref_v1, read_obj_v1, _) =
        get_past_obj_read_from_node(&node, object_id, object_ref_v1.1).await?;
    assert_eq!(read_ref_v1, object_ref_v1);
    assert_eq!(read_obj_v1, object_v1);
    assert_eq!(read_obj_v1.owner, Owner::AddressOwner(sender));

    let too_high_version = SequenceNumber::lamport_increment([object_ref_v3.1]);

    match node
        .state()
        .get_past_object_read(&object_id, too_high_version)?
    {
        PastObjectRead::VersionTooHigh {
            object_id: obj_id,
            asked_version,
            latest_version,
        } => {
            assert_eq!(obj_id, object_id);
            assert_eq!(asked_version, too_high_version);
            assert_eq!(latest_version, object_ref_v3.1);
        }
        other => anyhow::bail!(
            "Expect SequenceNumberTooHigh for object {object_id:?} but got {other:?}."
        ),
    };

    Ok(())
}

// Test for restoring a full node from a db snapshot
#[sim_test]
async fn test_full_node_bootstrap_from_snapshot() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10_000)
        .with_enable_db_checkpoints_fullnodes()
        .build()
        .await?;
    let checkpoint_path = test_cluster.fullnode_handle.sui_node.db_checkpoint_path();
    let config = test_cluster.fullnode_config_builder().build()?;
    let epoch_0_db_path = config.db_path().join("store").join("epoch_0");
    let context = &mut test_cluster.wallet;
    let _fullnode = test_cluster.fullnode_handle.sui_node.clone();
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let (_transferred_object, _, _, digest, ..) = transfer_coin(context).await?;

    // Skip the first epoch change from epoch 0 to epoch 1, but wait for the second
    // epoch change from epoch 1 to epoch 2 at which point during reconfiguration we will take
    // the db snapshot for epoch 1
    loop {
        if checkpoint_path.join("epoch_1").exists() {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    // Spin up a new full node restored from the snapshot taken at the end of epoch 1
    restore_from_db_checkpoint(&config, &checkpoint_path.join("epoch_1")).await?;
    let node = start_fullnode_from_config(config).await?.sui_node;

    wait_for_tx(digest, node.state().clone()).await;

    loop {
        // Ensure this full node is able to transition to the next epoch
        if node.current_epoch_for_testing() >= 2 {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    // Ensure this fullnode never processed older epoch (before snapshot) i.e. epoch_0 store was
    // doesn't exist
    assert!(!epoch_0_db_path.exists());

    let (_transferred_object, _, _, digest_after_restore, ..) = transfer_coin(context).await?;
    wait_for_tx(digest_after_restore, node.state().clone()).await;
    Ok(())
}
