// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{path::Path, str::FromStr};

use sui_config::utils::get_available_port;
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_core::test_utils::to_sender_signed_transaction;
use sui_framework_build::compiled_package::BuildConfig;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::TransactionExecutionApiClient;
use sui_json_rpc::api::{RpcReadApiClient, RpcTransactionBuilderClient};
use sui_json_rpc_types::{
    GetObjectDataResponse, SuiExecuteTransactionResponse, SuiTransactionResponse, TransactionBytes,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::object::Owner;
use sui_types::query::{EventQuery, TransactionQuery};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await?;

    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());
    Ok(())
}

#[tokio::test]
async fn test_public_transfer_object() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

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

    let tx_response: SuiExecuteTransactionResponse = http_client
        .execute_transaction(
            tx_bytes,
            sig_scheme,
            signature_bytes,
            pub_key,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    matches!(tx_response, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.mutated.len() == 2);

    Ok(())
}

#[tokio::test]
async fn test_publish() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    let compiled_modules = BuildConfig::default()
        .build(Path::new("../../sui_programmability/examples/fungible_tokens").to_path_buf())?
        .get_package_base64();

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            sig_scheme,
            signature_bytes,
            pub_key,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;
    matches!(tx_response, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.created.len() == 6);
    Ok(())
}

#[tokio::test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

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
        .execute_transaction(
            tx_bytes,
            sig_scheme,
            signature_bytes,
            pub_key,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;
    matches!(tx_response, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.created.len() == 1);
    Ok(())
}

#[tokio::test]
async fn test_get_object_info() -> Result<(), anyhow::Error> {
    let port = get_available_port();
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();
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
        .set_fullnode_rpc_port(port)
        .build()
        .await?;
    let http_client = cluster.rpc_client().unwrap();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas_id = objects.last().unwrap().object_id;

    // Make some transactions
    let mut tx_responses: Vec<SuiExecuteTransactionResponse> = Vec::new();
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
            .execute_transaction(
                tx_bytes,
                sig_scheme,
                signature_bytes,
                pub_key,
                ExecuteTransactionRequestType::WaitForLocalExecution,
            )
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
            |resp| matches!(resp, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.transaction_digest == response.effects.transaction_digest)
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
    let client = SuiClient::new(&format!("http://{}", addr), None, None).await?;
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
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(3), Some(true))
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());

    // test get all transactions paged
    let first_page = client
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(5), None)
        .await
        .unwrap();
    assert_eq!(5, first_page.data.len());
    assert!(first_page.next_cursor.is_some());

    // test get all transactions in ascending order
    let second_page = client
        .read_api()
        .get_transactions(TransactionQuery::All, first_page.next_cursor, None, None)
        .await
        .unwrap();
    assert_eq!(15, second_page.data.len());
    assert!(second_page.next_cursor.is_none());

    let mut all_txs_rev = first_page.data.clone();
    all_txs_rev.extend(second_page.data);
    all_txs_rev.reverse();

    // test get 10 latest transactions paged
    let latest = client
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(10), Some(true))
        .await
        .unwrap();
    assert_eq!(10, latest.data.len());

    assert_eq!(Some(all_txs_rev[10]), latest.next_cursor);
    assert_eq!(all_txs_rev[0..10], latest.data);

    // test get from address txs in ascending order
    let address_txs_asc = client
        .read_api()
        .get_transactions(
            TransactionQuery::FromAddress(cluster.accounts[0]),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(4, address_txs_asc.data.len());

    // test get from address txs in descending order
    let address_txs_desc = client
        .read_api()
        .get_transactions(
            TransactionQuery::FromAddress(cluster.accounts[0]),
            None,
            None,
            Some(true),
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
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(20), Some(true))
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

#[tokio::test]
async fn test_get_fullnode_events() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .set_fullnode_rpc_port(get_available_port())
        .build()
        .await
        .unwrap();
    let client = cluster.wallet.client;
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

    // test get all events ascending
    let page1 = client
        .event_api()
        .get_events(EventQuery::All, Some((2, 0).into()), Some(3), None)
        .await
        .unwrap();
    assert_eq!(3, page1.data.len());
    assert_eq!(Some((5, 0).into()), page1.next_cursor);
    let page2 = client
        .event_api()
        .get_events(EventQuery::All, Some((5, 0).into()), Some(20), None)
        .await
        .unwrap();
    assert_eq!(15, page2.data.len());
    assert_eq!(None, page2.next_cursor);

    // test get all events descending
    let page1 = client
        .event_api()
        .get_events(EventQuery::All, None, Some(3), Some(true))
        .await
        .unwrap();
    assert_eq!(3, page1.data.len());
    assert_eq!(Some((16, 0).into()), page1.next_cursor);
    let page2 = client
        .event_api()
        .get_events(EventQuery::All, Some((16, 0).into()), None, Some(true))
        .await
        .unwrap();
    assert_eq!(17, page2.data.len());
    assert_eq!(None, page2.next_cursor);

    // test get sender events
    let page = client
        .event_api()
        .get_events(
            EventQuery::Sender(cluster.accounts[0]),
            None,
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(4, page.data.len());

    // test get recipient events
    let page = client
        .event_api()
        .get_events(
            EventQuery::Recipient(Owner::AddressOwner(cluster.accounts[1])),
            None,
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(4, page.data.len());

    let object = client
        .read_api()
        .get_objects_owned_by_address(cluster.accounts[2])
        .await
        .unwrap()
        .last()
        .unwrap()
        .object_id;

    // test get object events
    let page = client
        .event_api()
        .get_events(EventQuery::Object(object), None, Some(10), None)
        .await
        .unwrap();
    assert_eq!(4, page.data.len());

    Ok(())
}
