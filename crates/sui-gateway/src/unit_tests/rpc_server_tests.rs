// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_package::BuildConfig;
use std::{path::Path, str::FromStr};
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_framework::build_move_package_to_bytes;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::{
    RpcGatewayApiClient, RpcReadApiClient, RpcTransactionBuilderClient, WalletSyncApiClient,
};
use sui_json_rpc_types::{GetObjectDataResponse, SuiTransactionResponse, TransactionBytes};
use sui_sdk::crypto::KeystoreType;
use sui_types::messages::Transaction;
use sui_types::sui_serde::Base64;
use sui_types::{
    base_types::{ObjectID, TransactionDigest},
    SUI_FRAMEWORK_ADDRESS,
};

use test_utils::network::start_rpc_test_network;

#[tokio::test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();

    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());
    Ok(())
}

#[tokio::test]
async fn test_public_transfer_object() -> Result<(), anyhow::Error> {
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
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

    let keystore_path = test_network.network.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = KeystoreType::File(keystore_path).init()?;

    let tx = Transaction::from_data(
        transaction_bytes.to_data().unwrap(),
        &keystore.signer(*address),
    );

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
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
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

    let keystore_path = test_network.network.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = KeystoreType::File(keystore_path).init()?;

    let tx = Transaction::from_data(
        transaction_bytes.to_data().unwrap(),
        &keystore.signer(*address),
    );

    let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let tx_response = http_client
        .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
        .await?;
    assert_eq!(6, tx_response.effects.created.len());
    Ok(())
}

#[tokio::test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    let package_id = ObjectID::new(SUI_FRAMEWORK_ADDRESS.into_bytes());
    let module = "object_basics".to_string();
    let function = "create".to_string();

    let json_args = vec![
        SuiJsonValue::from_str("10000")?,
        SuiJsonValue::from_str(&format!("{:#x}", address))?,
    ];

    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            package_id,
            module,
            function,
            vec![],
            json_args,
            Some(gas.object_id),
            1000,
        )
        .await?;

    let keystore_path = test_network.network.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = KeystoreType::File(keystore_path).init()?;

    let tx = Transaction::from_data(
        transaction_bytes.to_data().unwrap(),
        &keystore.signer(*address),
    );

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
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
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
    let test_network = start_rpc_test_network(None).await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();

    http_client.sync_account_state(*address).await?;

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas_id = objects.last().unwrap().object_id;

    // Make some transactions
    let mut tx_responses = Vec::new();
    for oref in &objects[..objects.len() - 1] {
        let transaction_bytes: TransactionBytes = http_client
            .transfer_object(*address, oref.object_id, Some(gas_id), 1000, *address)
            .await?;

        let keystore_path = test_network.network.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = KeystoreType::File(keystore_path).init()?;

        let tx = Transaction::from_data(
            transaction_bytes.to_data().unwrap(),
            &keystore.signer(*address),
        );

        let (tx_bytes, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

        let response = http_client
            .execute_transaction(tx_bytes, sig_scheme, signature_bytes, pub_key)
            .await?;

        tx_responses.push(response);
    }
    // test get_transactions_in_range
    let tx: Vec<(GatewayTxSeqNumber, TransactionDigest)> =
        http_client.get_transactions_in_range(0, 10).await?;
    assert_eq!(4, tx.len());

    // test get_transactions_in_range with smaller range
    let tx: Vec<(GatewayTxSeqNumber, TransactionDigest)> =
        http_client.get_transactions_in_range(1, 3).await?;
    assert_eq!(2, tx.len());

    // test get_recent_transactions with smaller range
    let tx: Vec<(GatewayTxSeqNumber, TransactionDigest)> =
        http_client.get_recent_transactions(3).await?;
    assert_eq!(3, tx.len());

    // test get_recent_transactions
    let tx: Vec<(GatewayTxSeqNumber, TransactionDigest)> =
        http_client.get_recent_transactions(10).await?;
    assert_eq!(4, tx.len());

    // test get_transaction
    for (_, tx_digest) in tx {
        let response: SuiTransactionResponse = http_client.get_transaction(tx_digest).await?;
        assert!(tx_responses.iter().any(
            |effects| effects.effects.transaction_digest == response.effects.transaction_digest
        ))
    }

    Ok(())
}
