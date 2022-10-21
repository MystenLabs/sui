// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{path::Path, str::FromStr};

use move_package::BuildConfig;

use sui_config::utils::get_available_port;
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_core::test_utils::to_sender_signed_transaction;
use sui_framework::build_move_package_to_bytes;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::{
    RpcGatewayApiClient, RpcReadApiClient, RpcTransactionBuilderClient, WalletSyncApiClient,
};
use sui_json_rpc_types::{GetObjectDataResponse, SuiTransactionResponse, TransactionBytes};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::query::{Ordering, TransactionQuery};
use sui_types::sui_serde::Base64;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;

    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());
    Ok(())
}

#[tokio::test]
async fn test_public_transfer_object() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;

    let transaction_bytes: TransactionBytes = http_client
        .transfer_object(
            *address,
            objects.first().unwrap().object_id,
            Some(objects.last().unwrap().object_id),
            1000,
            *address,
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let tx_response = http_client
        .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
        .await?;

    let effect = tx_response.effects;
    assert_eq!(2, effect.mutated.len());

    Ok(())
}

#[tokio::test]
async fn test_publish() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    let compiled_modules = build_move_package_to_bytes(
        Path::new("../../sui_programmability/examples/fungible_tokens"),
        BuildConfig::default(),
    )?
    .iter()
    .map(|bytes| Base64::from_bytes(bytes))
    .collect::<Vec<_>>();

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let tx_response = http_client
        .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
        .await?;
    assert_eq!(6, tx_response.effects.created.len());
    Ok(())
}

#[tokio::test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();
    let coin = &objects[1];

    // now do the call
    let package_id = ObjectID::new(SUI_FRAMEWORK_ADDRESS.into_bytes());
    let module = "pay".to_string();
    let function = "split".to_string();

    let json_args = vec![
        SuiJsonValue::from_object_id(coin.object_id),
        SuiJsonValue::from_str("10")?,
    ];

    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            package_id,
            module,
            function,
            vec![GAS::type_tag().into()],
            json_args,
            Some(gas.object_id),
            10_000,
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let tx_response = http_client
        .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
        .await?;
    let effect = tx_response.effects;
    assert_eq!(1, effect.created.len());
    Ok(())
}

#[tokio::test]
async fn test_get_object_info() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;

    for oref in objects {
        let result: GetObjectDataResponse = http_client.get_object(oref.object_id).await?;
        assert!(
            matches!(result, GetObjectDataResponse::Exists(object) if oref.object_id == object.id() && &object.owner.get_owner_address()? == address)
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_get_transaction() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_gateway_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

    http_client.sync_account_state(*address).await?;

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas_id = objects.last().unwrap().object_id;

    // Make some transactions
    let mut tx_responses = Vec::new();
    for oref in &objects[..objects.len() - 1] {
        let transaction_bytes: TransactionBytes = http_client
            .transfer_object(*address, oref.object_id, Some(gas_id), 1000, *address)
            .await?;
        let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

        let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

        let response = http_client
            .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
            .await?;

        tx_responses.push(response);
    }
    // test get_transactions_in_range
    let tx: Vec<TransactionDigest> = http_client.get_transactions_in_range(0, 10).await?;
    assert_eq!(4, tx.len());

    // test get_transactions_in_range with smaller range
    let tx: Vec<TransactionDigest> = http_client.get_transactions_in_range(1, 3).await?;
    assert_eq!(2, tx.len());

    // test get_transaction
    for tx_digest in tx {
        let response: SuiTransactionResponse = http_client.get_transaction(tx_digest).await?;
        assert!(tx_responses.iter().any(
            |effects| effects.effects.transaction_digest == response.effects.transaction_digest
        ))
    }

    Ok(())
}

#[tokio::test]
async fn test_get_fullnode_transaction() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await
        .unwrap();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let client = SuiClient::new_rpc_client(&format!("http://{}", addr), None).await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let mut tx_responses = Vec::new();

    for address in cluster.accounts.iter() {
        let objects = client
            .read_api()
            .get_objects_owned_by_address(*address)
            .await
            .unwrap();
        let gas_id = objects.last().unwrap().object_id;

        // Make some transactions
        for oref in &objects[..objects.len() - 1] {
            let data = client
                .transaction_builder()
                .transfer_object(*address, oref.object_id, Some(gas_id), 1000, *address)
                .await?;
            let tx = to_sender_signed_transaction(data, keystore.get_key(address).unwrap());

            let response = client
                .quorum_driver()
                .execute_transaction(
                    tx,
                    Some(ExecuteTransactionRequestType::WaitForLocalExecution),
                )
                .await
                .unwrap();

            tx_responses.push(response);
        }
    }

    // test get_recent_transactions with smaller range
    let tx = client
        .full_node_api()
        .get_transactions(TransactionQuery::All, None, Some(3), Ordering::Descending)
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());

    // test get all transactions paged
    let first_page = client
        .full_node_api()
        .get_transactions(TransactionQuery::All, None, Some(5), Ordering::Ascending)
        .await
        .unwrap();
    assert_eq!(5, first_page.data.len());
    assert!(first_page.next_cursor.is_some());

    // test get all transactions in ascending order
    let second_page = client
        .full_node_api()
        .get_transactions(
            TransactionQuery::All,
            first_page.next_cursor,
            None,
            Ordering::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(15, second_page.data.len());
    assert!(second_page.next_cursor.is_none());

    let mut all_txs_rev = first_page.data.clone();
    all_txs_rev.extend(second_page.data);
    all_txs_rev.reverse();

    // test get 10 latest transactions paged
    let latest = client
        .full_node_api()
        .get_transactions(TransactionQuery::All, None, Some(10), Ordering::Descending)
        .await
        .unwrap();
    assert_eq!(10, latest.data.len());

    assert_eq!(Some(all_txs_rev[10]), latest.next_cursor);
    assert_eq!(all_txs_rev[0..10], latest.data);

    // test get from address txs in ascending order
    let address_txs_asc = client
        .full_node_api()
        .get_transactions(
            TransactionQuery::FromAddress(cluster.accounts[0]),
            None,
            None,
            Ordering::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(4, address_txs_asc.data.len());

    // test get from address txs in descending order
    let address_txs_desc = client
        .full_node_api()
        .get_transactions(
            TransactionQuery::FromAddress(cluster.accounts[0]),
            None,
            None,
            Ordering::Descending,
        )
        .await
        .unwrap();
    assert_eq!(4, address_txs_desc.data.len());

    // test get from address txs in both ordering are the same.
    let mut data_asc = address_txs_asc.data;
    data_asc.reverse();
    assert_eq!(data_asc, address_txs_desc.data);

    // test get_recent_transactions
    let tx = client
        .full_node_api()
        .get_transactions(TransactionQuery::All, None, Some(20), Ordering::Descending)
        .await
        .unwrap();
    assert_eq!(20, tx.data.len());

    // test get_transaction
    for tx_digest in tx.data {
        let response: SuiTransactionResponse =
            client.read_api().get_transaction(tx_digest).await.unwrap();
        assert!(tx_responses.iter().any(|effects| effects
            .effects
            .as_ref()
            .unwrap()
            .transaction_digest
            == response.effects.transaction_digest))
    }

    Ok(())
}
