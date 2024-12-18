// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use move_core_types::annotated_value::MoveStructLayout;
use move_core_types::ident_str;
use rand::rngs::OsRng;
use std::path::PathBuf;
use std::sync::Arc;
use sui::client_commands::{OptsWithGas, SuiClientCommandResult, SuiClientCommands};
use sui_config::node::RunWithRange;
use sui_json_rpc_types::{EventFilter, TransactionFilter};
use sui_json_rpc_types::{
    EventPage, SuiEvent, SuiExecutionStatus, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_node::SuiNodeHandle;
use sui_sdk::wallet_context::WalletContext;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_test_transaction_builder::{
    batch_make_transfer_transactions, create_nft, delete_nft, increment_counter,
    publish_basics_package, publish_basics_package_and_make_counter, publish_nfts_package,
    TestTransactionBuilder,
};
use sui_tool::restore_from_db_checkpoint;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::crypto::{get_key_pair, SuiKeyPair};
use sui_types::error::{SuiError, UserInputError};
use sui_types::message_envelope::Message;
use sui_types::messages_grpc::TransactionInfoRequest;
use sui_types::object::{Object, ObjectRead, Owner, PastObjectRead};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequestType, ExecuteTransactionRequestV3, QuorumDriverResponse,
};
use sui_types::storage::ObjectStore;
use sui_types::transaction::{
    CallArg, GasData, TransactionData, TransactionKind, TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
    TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::{
    to_sender_signed_transaction, to_sender_signed_transaction_with_multi_signers,
};
use test_cluster::TestClusterBuilder;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::info;

#[sim_test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;

    let context = &mut test_cluster.wallet;

    // TODO: test fails on CI due to flakiness without this. Once https://github.com/MystenLabs/sui/pull/7056 is
    // merged we should be able to root out the flakiness.
    sleep(Duration::from_millis(10)).await;

    let (transferred_object, _, receiver, digest, _) = transfer_coin(context).await?;

    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;

    // A small delay is needed for post processing operations following the transaction to finish.
    sleep(Duration::from_secs(1)).await;

    // verify that the node has seen the transfer
    let object_read = fullnode.state().get_object_read(&transferred_object)?;
    let object = object_read.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    Ok(())
}

#[sim_test]
async fn test_full_node_shared_objects() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let handle = test_cluster.spawn_new_fullnode().await;

    let context = &mut test_cluster.wallet;

    let sender = context
        .config
        .keystore
        .addresses()
        .first()
        .cloned()
        .unwrap();
    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context).await;

    let response = increment_counter(
        context,
        sender,
        None,
        package_ref.0,
        counter_ref.0,
        counter_ref.1,
    )
    .await;
    let digest = response.digest;
    handle
        .sui_node
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;

    Ok(())
}

#[sim_test]
async fn test_sponsored_transaction() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let sender = test_cluster.get_address_0();
    let sponsor = test_cluster.get_address_1();
    let another_addr = test_cluster.get_address_2();

    // This makes sender send one coin to sponsor.
    // The sent coin is used as sponsor gas in the following sponsored tx.
    let (sent_coin, sender_, receiver, _, object_ref) =
        transfer_coin(&test_cluster.wallet).await.unwrap();
    assert_eq!(sender, sender_);
    assert_eq!(sponsor, receiver);
    let object_ref = test_cluster.wallet.get_object_ref(object_ref.0).await?;
    let gas_obj = test_cluster.wallet.get_object_ref(sent_coin).await?;
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
            test_cluster
                .wallet
                .config
                .keystore
                .get_key(&sender)
                .unwrap(),
            test_cluster
                .wallet
                .config
                .keystore
                .get_key(&sponsor)
                .unwrap(),
        ],
    );

    test_cluster.execute_transaction(tx).await;

    assert_eq!(
        sponsor,
        test_cluster
            .wallet
            .get_object_owner(&sent_coin)
            .await
            .unwrap(),
    );
    Ok(())
}

#[sim_test]
async fn test_full_node_move_function_index() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let node = &test_cluster.fullnode_handle.sui_node;
    let sender = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(context).await;
    let response = increment_counter(
        context,
        sender,
        None,
        package_ref.0,
        counter_ref.0,
        counter_ref.1,
    )
    .await;
    let digest = response.digest;

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::MoveFunction {
                package: package_ref.0,
                module: Some("counter".to_string()),
                function: Some("increment".to_string()),
            }),
            None,
            None,
            false,
        )
        .await?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::MoveFunction {
                package: package_ref.0,
                module: None,
                function: None,
            }),
            None,
            None,
            false,
        )
        .await?;

    // 2 transactions in the package i.e create and increment counter
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    eprint!("start...");
    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::MoveFunction {
                package: package_ref.0,
                module: Some("counter".to_string()),
                function: None,
            }),
            None,
            None,
            false,
        )
        .await?;

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
        .await;
    let node = &test_cluster.fullnode_handle.sui_node;
    let context = &mut test_cluster.wallet;

    let (transferred_object, sender, receiver, digest, _) = transfer_coin(context).await?;

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::InputObject(transferred_object)),
            None,
            None,
            false,
        )
        .await?;

    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::ChangedObject(transferred_object)),
            None,
            None,
            false,
        )
        .await?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::FromAddress(sender)),
            None,
            None,
            false,
        )
        .await?;
    assert_eq!(txes.len(), 1);
    assert_eq!(txes[0], digest);

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::ToAddress(receiver)),
            None,
            None,
            false,
        )
        .await?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    // Note that this is also considered a tx to the sender, because it mutated
    // one or more of the sender's objects.
    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::ToAddress(sender)),
            None,
            None,
            false,
        )
        .await?;
    assert_eq!(txes.len(), 2);
    assert_eq!(txes[1], digest);

    // No transactions have originated from the receiver
    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::FromAddress(receiver)),
            None,
            None,
            false,
        )
        .await?;
    assert_eq!(txes.len(), 0);

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
    let mut test_cluster = TestClusterBuilder::new().build().await;

    let context = &mut test_cluster.wallet;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let _ = transfer_coin(context).await?;
    let (_transferred_object, _, _, digest, ..) = transfer_coin(context).await?;

    // Make sure the validators are quiescent before bringing up the node.
    sleep(Duration::from_millis(1000)).await;

    // Start a new fullnode that is not on the write path
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;

    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;

    let info = fullnode
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
async fn test_full_node_sync_flood() {
    do_test_full_node_sync_flood().await
}

#[sim_test(check_determinism)]
async fn test_full_node_sync_flood_determinism() {
    do_test_full_node_sync_flood().await
}

async fn do_test_full_node_sync_flood() {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    // Start a new fullnode that is not on the write path
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;

    let context = test_cluster.wallet;

    let mut futures = Vec::new();

    let (package_ref, counter_ref) = publish_basics_package_and_make_counter(&context).await;

    let context = Arc::new(Mutex::new(context));

    // Start up 5 different tasks that all spam txs at the authorities.
    for _i in 0..5 {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let context = context.clone();
        tokio::task::spawn(async move {
            let (sender, object_to_split, gas_obj) = {
                let context = &mut context.lock().await;

                let sender = context
                    .config
                    .keystore
                    .addresses()
                    .first()
                    .cloned()
                    .unwrap();

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
                        opts: OptsWithGas::for_testing(
                            Some(gas_object_id),
                            TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN
                                * context.get_reference_gas_price().await.unwrap(),
                        ),
                    }
                    .execute(context)
                    .await
                    .unwrap()
                };

                owned_tx_digest = if let SuiClientCommandResult::TransactionBlock(resp) = res {
                    Some(resp.digest)
                } else {
                    panic!(
                        "SplitCoin command did not return SuiClientCommandResult::TransactionBlock"
                    );
                };

                let context = &context.lock().await;
                shared_tx_digest = Some(
                    increment_counter(
                        context,
                        sender,
                        Some(gas_object_id),
                        package_ref.0,
                        counter_ref.0,
                        counter_ref.1,
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
    let digests: Vec<_> = future::join_all(futures)
        .await
        .iter()
        .map(|r| r.clone().unwrap())
        .flat_map(|(a, b)| std::iter::once(a).chain(std::iter::once(b)))
        .collect();
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&digests)
        .await;
}

// Test fullnode has event read jsonrpc endpoints working
#[sim_test]
async fn test_full_node_event_read_api_ok() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_fullnode_rpc_port(50000)
        .enable_fullnode_events()
        .build()
        .await;

    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.sui_node;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let (package_id, gas_id_1, _) = publish_nfts_package(context).await;

    let (transferred_object, _, _, digest, _) = transfer_coin(context).await.unwrap();

    let txes = node
        .state()
        .get_transactions_for_tests(
            Some(TransactionFilter::InputObject(transferred_object)),
            None,
            None,
            false,
        )
        .await
        .unwrap();

    if gas_id_1 == transferred_object {
        assert_eq!(txes.len(), 2);
        assert!(txes[0] == digest || txes[1] == digest);
    } else {
        assert_eq!(txes.len(), 1);
        assert_eq!(txes[0], digest);
    }

    // This is a poor substitute for the post processing taking some time
    sleep(Duration::from_millis(1000)).await;

    let (_sender, _object_id, digest2) = create_nft(context, package_id).await;

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
async fn test_full_node_event_query_by_module_ok() {
    let mut test_cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await;

    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let (package_id, _, _) = publish_nfts_package(context).await;

    // This is a poor substitute for the post processing taking some time
    sleep(Duration::from_millis(1000)).await;

    let (_sender, _object_id, digest2) = create_nft(context, package_id).await;

    // Add a delay to ensure event processing is done after transaction commits.
    sleep(Duration::from_secs(5)).await;

    // query by move event module
    let params = rpc_params![EventFilter::MoveEventModule {
        package: package_id,
        module: ident_str!("testnet_nft").into()
    }];
    let page: EventPage = jsonrpc_client
        .request("suix_queryEvents", params)
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].id.tx_digest, digest2);
}

#[sim_test]
async fn test_full_node_transaction_orchestrator_basic() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    let metrics = KeyValueStoreMetrics::new_for_tests();
    let kv_store = Arc::new(TransactionKeyValueStore::new(
        "rocksdb",
        metrics,
        fullnode.state(),
    ));

    let context = &mut test_cluster.wallet;
    let transaction_orchestrator = fullnode.with(|node| {
        node.transaction_orchestrator()
            .expect("Fullnode should have transaction orchestrator toggled on.")
    });
    let mut rx = fullnode.with(|node| {
        node.subscribe_to_transaction_orchestrator_effects()
            .expect("Fullnode should have transaction orchestrator toggled on.")
    });

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // Test WaitForLocalExecution
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction_block(
            ExecuteTransactionRequestV3::new_v2(txn),
            ExecuteTransactionRequestType::WaitForLocalExecution,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let (
        tx,
        QuorumDriverResponse {
            effects_cert: certified_txn_effects,
            events: txn_events,
            ..
        },
    ) = rx.recv().await.unwrap().unwrap();
    let (response, is_executed_locally) = res;
    assert_eq!(*tx.digest(), digest);
    assert_eq!(
        response.effects.effects.digest(),
        *certified_txn_effects.digest()
    );
    assert!(is_executed_locally);
    assert_eq!(
        response.events.unwrap_or_default().digest(),
        txn_events.unwrap_or_default().digest()
    );
    // verify that the node has sequenced and executed the txn
    fullnode.state().get_executed_transaction_and_effects(digest, kv_store.clone()).await
        .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForLocalExecution: {:?}", digest, e));

    // Test WaitForEffectsCert
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = transaction_orchestrator
        .execute_transaction_block(
            ExecuteTransactionRequestV3::new_v2(txn),
            ExecuteTransactionRequestType::WaitForEffectsCert,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let (
        tx,
        QuorumDriverResponse {
            effects_cert: certified_txn_effects,
            events: txn_events,
            ..
        },
    ) = rx.recv().await.unwrap().unwrap();
    let (response, is_executed_locally) = res;
    assert_eq!(*tx.digest(), digest);
    assert_eq!(
        response.effects.effects.digest(),
        *certified_txn_effects.digest()
    );
    assert_eq!(
        txn_events.unwrap_or_default().digest(),
        response.events.unwrap_or_default().digest()
    );
    assert!(!is_executed_locally);
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;
    fullnode.state().get_executed_transaction_and_effects(digest, kv_store).await
        .unwrap_or_else(|e| panic!("Fullnode does not know about the txn {:?} that was executed with WaitForEffectsCert: {:?}", digest, e));

    Ok(())
}

/// Test a validator node does not have transaction orchestrator
#[tokio::test]
async fn test_validator_node_has_no_transaction_orchestrator() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let node_handle = test_cluster.swarm.validator_node_handles().pop().unwrap();
    node_handle.with(|node| {
        assert!(node.transaction_orchestrator().is_none());
        assert!(node
            .subscribe_to_transaction_orchestrator_effects()
            .is_err());
    });
}

#[sim_test]
async fn test_execute_tx_with_serialized_signature() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    context
        .config
        .keystore
        .add_key(None, SuiKeyPair::Secp256k1(get_key_pair().1))?;
    context
        .config
        .keystore
        .add_key(None, SuiKeyPair::Ed25519(get_key_pair().1))?;

    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let txn_count = 4;
    let txns = batch_make_transfer_transactions(context, txn_count).await;
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
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
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
    node: &SuiNodeHandle,
    object_id: ObjectID,
) -> Result<(ObjectRef, Object, Option<MoveStructLayout>), anyhow::Error> {
    if let ObjectRead::Exists(obj_ref, object, layout) = node.state().get_object_read(&object_id)? {
        Ok((obj_ref, object, layout))
    } else {
        anyhow::bail!("Can't find object {object_id:?} on fullnode.")
    }
}

async fn get_past_obj_read_from_node(
    node: &SuiNodeHandle,
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
    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let node = &test_cluster.fullnode_handle.sui_node;
    let package_id = publish_nfts_package(&test_cluster.wallet).await.0;

    // Create the object
    let (sender, object_id, _) = create_nft(&test_cluster.wallet, package_id).await;

    let recipient = test_cluster.get_address_1();
    assert_ne!(sender, recipient);

    let (object_ref_v1, object_v1, _) = get_obj_read_from_node(node, object_id).await?;

    // Transfer the object from sender to recipient
    let gas_ref = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let nft_transfer_tx = test_cluster.wallet.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_ref, rgp)
            .transfer(object_ref_v1, recipient)
            .build(),
    );
    test_cluster.execute_transaction(nft_transfer_tx).await;
    sleep(Duration::from_secs(1)).await;

    let (object_ref_v2, object_v2, _) = get_obj_read_from_node(node, object_id).await?;
    assert_ne!(object_ref_v2, object_ref_v1);

    // Transfer some SUI to recipient
    transfer_coin(&test_cluster.wallet)
        .await
        .expect("Failed to transfer coins to recipient");

    // Delete the object
    let response = delete_nft(&test_cluster.wallet, recipient, package_id, object_ref_v2).await;
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
        get_past_obj_read_from_node(node, object_id, object_ref_v2.1).await?;
    assert_eq!(read_ref_v2, object_ref_v2);
    assert_eq!(read_obj_v2, object_v2);
    assert_eq!(read_obj_v2.owner, Owner::AddressOwner(recipient));

    let (read_ref_v1, read_obj_v1, _) =
        get_past_obj_read_from_node(node, object_id, object_ref_v1.1).await?;
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
        // This will also do aggressive pruning and compaction of the snapshot
        .with_enable_db_checkpoints_fullnodes()
        .build()
        .await;

    let checkpoint_path = test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.db_checkpoint_path());
    let config = test_cluster
        .fullnode_config_builder()
        .build(&mut OsRng, test_cluster.swarm.config());
    let epoch_0_db_path = config.db_path().join("store").join("epoch_0");
    let _ = transfer_coin(&test_cluster.wallet).await?;
    let _ = transfer_coin(&test_cluster.wallet).await?;
    let (_transferred_object, _, _, digest, ..) = transfer_coin(&test_cluster.wallet).await?;

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
    let node = test_cluster
        .start_fullnode_from_config(config)
        .await
        .sui_node;

    node.state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;

    loop {
        // Ensure this full node is able to transition to the next epoch
        if node.with(|node| node.current_epoch_for_testing()) >= 2 {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    // Ensure this fullnode never processed older epoch (before snapshot) i.e. epoch_0 store was
    // doesn't exist
    assert!(!epoch_0_db_path.exists());

    let (_transferred_object, _, _, digest_after_restore, ..) =
        transfer_coin(&test_cluster.wallet).await?;
    node.state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest_after_restore])
        .await;
    Ok(())
}

// Object fast path should be disabled and unused.
#[sim_test]
async fn test_pass_back_no_object() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;

    let context = &mut test_cluster.wallet;

    let sender = context
        .config
        .keystore
        .addresses()
        .first()
        .cloned()
        .unwrap();

    // TODO: this is publishing the wrong package - we should be publishing the one in `sui-core/src/unit_tests/data` instead.
    let package_ref = publish_basics_package(context).await;

    let gas_obj = context
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();

    let transaction_orchestrator = fullnode.with(|node| {
        node.transaction_orchestrator()
            .expect("Fullnode should have transaction orchestrator toggled on.")
    });
    let mut rx = fullnode.with(|node| {
        node.subscribe_to_transaction_orchestrator_effects()
            .expect("Fullnode should have transaction orchestrator toggled on.")
    });

    let tx_data = TransactionData::new_move_call(
        sender,
        package_ref.0,
        ident_str!("object_basics").to_owned(),
        ident_str!("use_clock").to_owned(),
        /* type_args */ vec![],
        gas_obj,
        vec![CallArg::CLOCK_IMM],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    let tx =
        to_sender_signed_transaction(tx_data, context.config.keystore.get_key(&sender).unwrap());

    let digest = *tx.digest();
    let _res = transaction_orchestrator
        .execute_transaction_block(
            ExecuteTransactionRequestV3::new_v2(tx),
            ExecuteTransactionRequestType::WaitForLocalExecution,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));
    println!("res: {:?}", _res);

    let (
        _tx,
        QuorumDriverResponse {
            effects_cert: _certified_txn_effects,
            events: _txn_events,
            ..
        },
    ) = rx.recv().await.unwrap().unwrap();
    Ok(())
}

#[sim_test]
async fn test_access_old_object_pruned() {
    // This test checks that when we ask a validator to handle a transaction that uses
    // an old object that's already been pruned, it's able to return an non-retriable
    // error ObjectVersionUnavailableForConsumption, instead of the retriable error
    // ObjectNotFound.
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx_builder = test_cluster.test_transaction_builder().await;
    let sender = tx_builder.sender();
    let gas_object = tx_builder.gas_object();
    let effects = test_cluster
        .sign_and_execute_transaction(&tx_builder.transfer_sui(None, sender).build())
        .await
        .effects
        .unwrap();
    let new_gas_version = effects.gas_object().reference.version;
    test_cluster.trigger_reconfiguration().await;
    // Construct a new transaction that uses the old gas object reference.
    let tx = test_cluster.sign_transaction(
        &test_cluster
            .test_transaction_builder_with_gas_object(sender, gas_object)
            .await
            // Make sure we are doing something different from the first transaction.
            // Otherwise we would just end up with the same digest.
            .transfer_sui(Some(1), sender)
            .build(),
    );
    for validator in test_cluster.swarm.active_validators() {
        validator
            .get_node_handle()
            .unwrap()
            .with_async(|node| async {
                let state = node.state();
                state
                    .database_for_testing()
                    .prune_objects_and_compact_for_testing(
                        state.get_checkpoint_store(),
                        state.rpc_index.as_deref(),
                    )
                    .await;
                // Make sure the old version of the object is already pruned.
                assert!(state
                    .database_for_testing()
                    .get_object_by_key(&gas_object.0, gas_object.1)
                    .is_none());
                let epoch_store = state.epoch_store_for_testing();
                assert_eq!(
                    state
                        .handle_transaction(
                            &epoch_store,
                            epoch_store.verify_transaction(tx.clone()).unwrap()
                        )
                        .await
                        .unwrap_err(),
                    SuiError::UserInputError {
                        error: UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: gas_object,
                            current_version: new_gas_version,
                        }
                    }
                );
            })
            .await;
    }

    // Check that fullnode would return the same error.
    let result = test_cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(result.unwrap_err().to_string().contains(
        &UserInputError::ObjectVersionUnavailableForConsumption {
            provided_obj_ref: gas_object,
            current_version: new_gas_version,
        }
        .to_string()
    ))
}

async fn transfer_coin(
    context: &WalletContext,
) -> Result<
    (
        ObjectID,
        SuiAddress,
        SuiAddress,
        TransactionDigest,
        ObjectRef,
    ),
    anyhow::Error,
> {
    let gas_price = context.get_reference_gas_price().await?;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .transfer(object_to_send, receiver)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    Ok((object_to_send.0, sender, receiver, resp.digest, gas_object))
}

#[sim_test]
async fn test_full_node_run_with_range_checkpoint() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let stop_after_checkpoint_seq = 5;
    let want_run_with_range = Some(RunWithRange::Checkpoint(stop_after_checkpoint_seq));
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10_000)
        .with_fullnode_run_with_range(want_run_with_range)
        .build()
        .await;

    // wait for node to signal that we reached and processed our desired epoch
    let got_run_with_range = test_cluster.wait_for_run_with_range_shutdown_signal().await;

    // ensure we got the expected RunWithRange on shutdown channel
    assert_eq!(got_run_with_range, want_run_with_range);

    // ensure the highest synced checkpoint matches
    assert!(test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            == Some(stop_after_checkpoint_seq)
    }));

    // sleep some time to ensure we don't see further ccheckpoints executed
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // verify again execution has not progressed beyond expectations
    assert!(test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            == Some(stop_after_checkpoint_seq)
    }));

    // we dont want transaction orchestrator enabled when run_with_range != None
    assert!(test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.transaction_orchestrator())
        .is_none());
    Ok(())
}

#[sim_test]
async fn test_full_node_run_with_range_epoch() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let stop_after_epoch = 2;
    let want_run_with_range = Some(RunWithRange::Epoch(stop_after_epoch));
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10_000)
        .with_fullnode_run_with_range(want_run_with_range)
        .build()
        .await;

    // wait for node to signal that we reached and processed our desired epoch
    let got_run_with_range = test_cluster.wait_for_run_with_range_shutdown_signal().await;

    // ensure we get the shutdown signal
    assert_eq!(got_run_with_range, want_run_with_range);

    // ensure we end up at epoch + 1
    // this is because we execute the target epoch, reconfigure, and then send shutdown signal at
    // epoch + 1
    assert!(test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.current_epoch_for_testing() == stop_after_epoch + 1));

    // epoch duration is 10s for testing, lets sleep long enough that epoch would normally progress
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // ensure we are still at epoch + 1
    assert!(test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.current_epoch_for_testing() == stop_after_epoch + 1));

    // we dont want transaction orchestrator enabled when run_with_range != None
    assert!(test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.transaction_orchestrator())
        .is_none());

    Ok(())
}

// This test checks that the fullnode is able to resolve events emitted from a transaction
// that references the structs defined in the package published by the transaction itself,
// without local execution.
#[sim_test]
async fn publish_init_events_without_local_execution() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/move_test_code");
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let tx = test_cluster.sign_transaction(&tx_data);
    let client = test_cluster.wallet.get_client().await.unwrap();
    let response = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_events(),
            Some(ExecuteTransactionRequestType::WaitForEffectsCert),
        )
        .await
        .unwrap();
    assert_eq!(response.events.unwrap().data.len(), 1);
}
