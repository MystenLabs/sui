// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::api::CoinReadApiClient;
use crate::api::GovernanceReadApiClient;
use crate::api::{RpcFullNodeReadApiClient, ThresholdBlsApiClient, TransactionExecutionApiClient};
use crate::api::{RpcReadApiClient, RpcTransactionBuilderClient};
use std::path::Path;

#[cfg(not(msim))]
use std::str::FromStr;
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_framework_build::compiled_package::BuildConfig;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::SuiObjectInfo;
use sui_json_rpc_types::{
    Balance, CoinPage, GetObjectDataResponse, SuiCoinMetadata, SuiEvent,
    SuiExecuteTransactionResponse, SuiExecutionStatus, SuiTBlsSignObjectCommitmentType,
    SuiTransactionResponse, TransactionBytes,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_types::balance::Supply;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::coin::{TreasuryCap, COIN_MODULE_NAME, LOCKED_COIN_MODULE_NAME};
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::object::Owner;
use sui_types::query::{EventQuery, TransactionQuery};
use sui_types::sui_system_state::ValidatorMetadata;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_struct_tag, parse_sui_type_tag, SUI_FRAMEWORK_ADDRESS};
use test_utils::network::TestClusterBuilder;

use sui_macros::sim_test;
use sui_types::governance::{DelegatedStake, DelegationStatus};

use tokio::time::{sleep, Duration};

#[sim_test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());
    Ok(())
}

#[sim_test]
async fn test_public_transfer_object() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
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
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();
    let tx_bytes1 = tx_bytes.clone();
    let dryrun_response = http_client.dry_run_transaction(tx_bytes).await?;

    let tx_response: SuiExecuteTransactionResponse = http_client
        .execute_transaction(
            tx_bytes1,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;
    assert_eq!(
        dryrun_response.transaction_digest,
        effects.effects.transaction_digest
    );
    Ok(())
}

#[sim_test]
async fn test_tbls_sign_randomness_object() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);

    let address = cluster.accounts.first().unwrap();
    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    ////////////////////////////////////////////////////////////////////////
    // Trying to get a tBLS signature for a type that is not Randomness should fail.

    let tx_response = http_client
        .tbls_sign_randomness_object(
            gas.object_id,
            SuiTBlsSignObjectCommitmentType::ConsensusCommitted,
        )
        .await;
    assert!(tx_response.is_err());

    ////////////////////////////////////////////////////////////////////////
    // Publish the basic randomness example

    let compiled_modules = BuildConfig::default()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?
        .get_package_base64(false);
    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;
    assert_eq!(SuiExecutionStatus::Success, effects.effects.status);

    let package_id = effects
        .effects
        .events
        .iter()
        .find_map(|e| {
            if let SuiEvent::Publish { package_id, .. } = e {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();

    ////////////////////////////////////////////////////////////////////////
    // Call create_owned_randomness

    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            *package_id,
            "randomness_basics".to_string(),
            "create_owned_randomness".to_string(),
            vec![],
            vec![],
            Some(gas.object_id),
            10_000,
            None,
        )
        .await?;
    let tx = transaction_bytes.to_data()?;
    let tx = to_sender_signed_transaction(tx, keystore.get_key(address)?);
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForEffectsCert,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;
    assert_eq!(SuiExecutionStatus::Success, effects.effects.status);

    let randomness_object_id = effects
        .effects
        .events
        .iter()
        .find_map(|e| {
            if let SuiEvent::NewObject { object_id, .. } = e {
                Some(*object_id)
            } else {
                None
            }
        })
        .unwrap();

    ////////////////////////////////////////////////////////////////////////
    // Get the tBLS signature from the JSON-RPC.

    let tx_response = http_client
        .tbls_sign_randomness_object(
            randomness_object_id,
            SuiTBlsSignObjectCommitmentType::ConsensusCommitted,
        )
        .await?;

    let sig = tx_response.signature.0;

    ////////////////////////////////////////////////////////////////////////
    // Call set_randomness

    let transaction_bytes = http_client
        .move_call(
            *address,
            *package_id,
            "randomness_basics".to_string(),
            "set_randomness".to_string(),
            vec![],
            vec![
                SuiJsonValue::from_object_id(randomness_object_id),
                SuiJsonValue::from_bcs_bytes(&sig).unwrap(),
            ],
            Some(gas.object_id),
            10_000,
            None,
        )
        .await?;
    let tx = transaction_bytes.to_data()?;
    let tx = to_sender_signed_transaction(tx, keystore.get_key(address)?);
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForEffectsCert,
        )
        .await?;
    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;
    assert_eq!(SuiExecutionStatus::Success, effects.effects.status);

    Ok(())
}

#[sim_test]
async fn test_publish() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    let compiled_modules = BuildConfig::default()
        .build(Path::new("../../sui_programmability/examples/fungible_tokens").to_path_buf())?
        .get_package_base64(/* with_unpublished_deps */ false);

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;
    matches!(tx_response, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.created.len() == 6);
    Ok(())
}

#[sim_test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
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
        SuiJsonValue::from_str("\"10\"")?,
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
            None,
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;
    matches!(tx_response, SuiExecuteTransactionResponse::EffectsCert {effects, ..} if effects.effects.created.len() == 1);
    Ok(())
}

#[sim_test]
async fn test_get_object_info() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
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

#[sim_test]
async fn test_get_coins() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let result: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    assert_eq!(5, result.data.len());
    assert_eq!(None, result.next_cursor);

    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::TestCoin".into()), None, None)
        .await?;
    assert_eq!(0, result.data.len());

    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::SUI".into()), None, None)
        .await?;
    assert_eq!(5, result.data.len());
    assert_eq!(None, result.next_cursor);

    // Test paging
    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::SUI".into()), None, Some(3))
        .await?;
    assert_eq!(3, result.data.len());
    assert!(result.next_cursor.is_some());

    let result: CoinPage = http_client
        .get_coins(
            *address,
            Some("0x2::sui::SUI".into()),
            result.next_cursor,
            Some(3),
        )
        .await?;
    assert_eq!(2, result.data.len());
    assert!(result.next_cursor.is_none());

    Ok(())
}

#[sim_test]
async fn test_get_balance() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let result: Balance = http_client.get_balance(*address, None).await?;
    assert_eq!("0x2::sui::SUI", result.coin_type);
    assert_eq!(500000000000000, result.total_balance);
    assert_eq!(5, result.coin_object_count);

    Ok(())
}

#[sim_test]
async fn test_get_metadata() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    // Publish test coin package
    let compiled_modules = BuildConfig::default()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?
        .get_package_base64(/* with_unpublished_deps */ false);

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signature) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;

    let package_id = effects
        .effects
        .events
        .iter()
        .find_map(|e| {
            if let SuiEvent::Publish { package_id, .. } = e {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();

    let result: SuiCoinMetadata = http_client
        .get_coin_metadata(format!("{package_id}::trusted_coin::TRUSTED_COIN"))
        .await?;

    assert_eq!("TRUSTED", result.symbol);
    assert_eq!("Trusted Coin for test", result.description);
    assert_eq!("Trusted Coin", result.name);
    assert_eq!(2, result.decimals);

    Ok(())
}

#[sim_test]
async fn test_get_total_supply() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    let gas = objects.first().unwrap();

    // Publish test coin package
    let compiled_modules = BuildConfig::default()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?
        .get_package_base64(/* with_unpublished_deps */ false);

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signature) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;

    let package_id = effects
        .effects
        .events
        .iter()
        .find_map(|e| {
            if let SuiEvent::Publish { package_id, .. } = e {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();

    let coin_name = format!("{package_id}::trusted_coin::TRUSTED_COIN");
    let coin_type = parse_sui_type_tag(&coin_name).unwrap();
    let result: Supply = http_client.get_total_supply(coin_name.clone()).await?;

    assert_eq!(0, result.value);

    let treasury_cap = effects
        .effects
        .events
        .iter()
        .find_map(|e| {
            if let SuiEvent::NewObject {
                object_id,
                object_type,
                ..
            } = e
            {
                if TreasuryCap::type_(parse_sui_struct_tag(&coin_name).unwrap())
                    == parse_sui_struct_tag(object_type).unwrap()
                {
                    Some(object_id)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    // Mint 100000 coin

    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            SUI_FRAMEWORK_ADDRESS.into(),
            COIN_MODULE_NAME.to_string(),
            "mint_and_transfer".into(),
            vec![coin_type.into()],
            vec![
                SuiJsonValue::from_str(&treasury_cap.to_string()).unwrap(),
                SuiJsonValue::from_str("\"100000\"").unwrap(),
                SuiJsonValue::from_str(&address.to_string()).unwrap(),
            ],
            Some(gas.object_id),
            10_000,
            None,
        )
        .await?;

    let tx = transaction_bytes.to_data()?;

    let tx = to_sender_signed_transaction(tx, keystore.get_key(address)?);
    let (tx_bytes, signature) = tx.to_tx_bytes_and_signature();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signature,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let SuiExecuteTransactionResponse::EffectsCert { effects, .. } = tx_response;

    assert_eq!(SuiExecutionStatus::Success, effects.effects.status);

    let result: Supply = http_client.get_total_supply(coin_name.clone()).await?;
    assert_eq!(100000, result.value);

    Ok(())
}

#[sim_test]
async fn test_get_transaction() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
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

        let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

        let response = http_client
            .execute_transaction(
                tx_bytes,
                signature_bytes,
                ExecuteTransactionRequestType::WaitForLocalExecution,
            )
            .await?;

        tx_responses.push(response);
    }
    // test get_transactions_in_range
    let tx: Vec<TransactionDigest> = http_client.get_transactions_in_range(0, 10).await?;
    assert_eq!(5, tx.len());

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

#[sim_test]
async fn test_get_fullnode_transaction() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await.unwrap();

    let context = &mut cluster.wallet;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let mut tx_responses = Vec::new();

    let client = context.get_client().await.unwrap();

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
        .get_transactions(TransactionQuery::All, None, Some(3), true)
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());

    // test get all transactions paged
    let first_page = client
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(5), false)
        .await
        .unwrap();
    assert_eq!(5, first_page.data.len());
    assert!(first_page.next_cursor.is_some());

    // test get all transactions in ascending order
    let second_page = client
        .read_api()
        .get_transactions(TransactionQuery::All, first_page.next_cursor, None, false)
        .await
        .unwrap();
    assert_eq!(16, second_page.data.len());
    assert!(second_page.next_cursor.is_none());

    let mut all_txs_rev = first_page.data.clone();
    all_txs_rev.extend(second_page.data);
    all_txs_rev.reverse();

    // test get 10 latest transactions paged
    let latest = client
        .read_api()
        .get_transactions(TransactionQuery::All, None, Some(10), true)
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
            false,
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
            true,
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
        .get_transactions(TransactionQuery::All, None, Some(20), true)
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

#[sim_test]
async fn test_get_fullnode_events() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await
        .unwrap();
    let client = cluster.wallet.get_client().await?;
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

    // Add a delay to ensure event processing is done after transaction commits.
    sleep(Duration::from_millis(100)).await;

    // test get all events ascending
    let page1 = client
        .event_api()
        .get_events(
            EventQuery::All,
            Some((tx_responses[2].tx_digest, 0).into()),
            Some(3),
            false,
        )
        .await
        .unwrap();
    assert_eq!(3, page1.data.len());
    assert_eq!(
        Some((tx_responses[5].tx_digest, 0).into()),
        page1.next_cursor
    );
    let page2 = client
        .event_api()
        .get_events(
            EventQuery::All,
            Some((tx_responses[5].tx_digest, 0).into()),
            Some(20),
            false,
        )
        .await
        .unwrap();
    assert_eq!(15, page2.data.len());
    assert_eq!(None, page2.next_cursor);

    // test get all events descending
    let page1 = client
        .event_api()
        .get_events(EventQuery::All, None, Some(3), true)
        .await
        .unwrap();
    assert_eq!(3, page1.data.len());
    assert_eq!(
        Some((tx_responses[16].tx_digest, 0).into()),
        page1.next_cursor
    );

    let page2 = client
        .event_api()
        .get_events(
            EventQuery::All,
            Some((tx_responses[16].tx_digest, 0).into()),
            None,
            true,
        )
        .await
        .unwrap();
    // 17 events created by this test + 33 Genesis event
    assert_eq!(50, page2.data.len());
    assert_eq!(None, page2.next_cursor);

    // test get sender events
    let page = client
        .event_api()
        .get_events(
            EventQuery::Sender(cluster.accounts[0]),
            None,
            Some(10),
            false,
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
            false,
        )
        .await
        .unwrap();
    assert_eq!(9, page.data.len());

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
        .get_events(EventQuery::Object(object), None, Some(10), false)
        .await
        .unwrap();
    assert_eq!(5, page.data.len());

    Ok(())
}

#[sim_test]
async fn test_locked_sui() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());
    // verify coins and balance before test
    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    let balance: Vec<Balance> = http_client.get_all_balances(*address).await?;

    assert_eq!(5, coins.data.len());
    for coin in &coins.data {
        assert!(coin.locked_until_epoch.is_none());
    }

    assert_eq!(1, balance.len());
    assert!(balance[0].locked_balance.is_empty());

    // lock one coin
    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            SUI_FRAMEWORK_ADDRESS.into(),
            LOCKED_COIN_MODULE_NAME.to_string(),
            "lock_coin".to_string(),
            vec![parse_sui_type_tag("0x2::sui::SUI")?.into()],
            vec![
                SuiJsonValue::from_str(&coins.data[0].coin_object_id.to_string())?,
                SuiJsonValue::from_str(&format!("{address}"))?,
                SuiJsonValue::from_bcs_bytes(&bcs::to_bytes(&"20")?)?,
            ],
            None,
            1000,
            None,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let balances: Vec<Balance> = http_client.get_all_balances(*address).await?;

    assert_eq!(1, balance.len());

    let balance = balances.first().unwrap();

    assert_eq!(5, balance.coin_object_count);
    assert_eq!(1, balance.locked_balance.len());
    assert!(balance.locked_balance.contains_key(&20));

    Ok(())
}

#[sim_test]
async fn test_delegation() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects: Vec<SuiObjectInfo> = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());

    // Check StakedSui object before test
    let staked_sui: Vec<DelegatedStake> = http_client.get_delegated_stakes(*address).await?;
    assert!(staked_sui.is_empty());

    let validators: Vec<ValidatorMetadata> = http_client.get_validators().await?;

    // Delegate some SUI
    let transaction_bytes: TransactionBytes = http_client
        .request_add_delegation(
            *address,
            vec![objects[0].object_id],
            Some(1000000),
            validators[0].sui_address,
            None,
            10000,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_delegated_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000, staked_sui[0].staked_sui.principal());
    assert!(staked_sui[0].staked_sui.sui_token_lock().is_none());
    assert!(matches!(
        staked_sui[0].delegation_status,
        DelegationStatus::Pending
    ));
    Ok(())
}

#[sim_test]
async fn test_delegation_multiple_coins() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    assert_eq!(5, coins.data.len());

    let genesis_coin_amount = coins.data[0].balance;

    // Check StakedSui object before test
    let staked_sui: Vec<DelegatedStake> = http_client.get_delegated_stakes(*address).await?;
    assert!(staked_sui.is_empty());

    let validators: Vec<ValidatorMetadata> = http_client.get_validators().await?;

    // Delegate some SUI
    let transaction_bytes: TransactionBytes = http_client
        .request_add_delegation(
            *address,
            vec![
                coins.data[0].coin_object_id,
                coins.data[1].coin_object_id,
                coins.data[2].coin_object_id,
            ],
            Some(1000000),
            validators[0].sui_address,
            None,
            10000,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_delegated_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000, staked_sui[0].staked_sui.principal());
    assert!(staked_sui[0].staked_sui.sui_token_lock().is_none());
    assert!(matches!(
        staked_sui[0].delegation_status,
        DelegationStatus::Pending
    ));

    // Coins should be merged into one and returned to the sender.
    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    assert_eq!(3, coins.data.len());

    // Find the new coin
    let new_coin = coins
        .data
        .iter()
        .find(|coin| coin.balance > genesis_coin_amount)
        .unwrap();
    assert_eq!((genesis_coin_amount * 3) - 1000000, new_coin.balance);

    Ok(())
}

#[sim_test]
async fn test_delegation_with_locked_sui() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects: Vec<SuiObjectInfo> = http_client.get_objects_owned_by_address(*address).await?;
    assert_eq!(5, objects.len());

    // lock some SUI
    let transaction_bytes: TransactionBytes = http_client
        .move_call(
            *address,
            SUI_FRAMEWORK_ADDRESS.into(),
            LOCKED_COIN_MODULE_NAME.to_string(),
            "lock_coin".to_string(),
            vec![parse_sui_type_tag("0x2::sui::SUI")?.into()],
            vec![
                SuiJsonValue::from_str(&objects[0].object_id.to_string())?,
                SuiJsonValue::from_str(&format!("{address}"))?,
                SuiJsonValue::from_bcs_bytes(&bcs::to_bytes(&"20")?)?,
            ],
            None,
            1000,
            None,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    let validators: Vec<ValidatorMetadata> = http_client.get_validators().await?;

    // Delegate some locked SUI
    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    let locked_sui = coins
        .data
        .iter()
        .find_map(|coin| coin.locked_until_epoch.map(|_| coin.coin_object_id))
        .unwrap();

    let transaction_bytes: TransactionBytes = http_client
        .request_add_delegation(
            *address,
            vec![locked_sui],
            Some(1000000),
            validators[0].sui_address,
            None,
            10000,
        )
        .await?;
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signature_bytes) = tx.to_tx_bytes_and_signature();

    http_client
        .execute_transaction(
            tx_bytes,
            signature_bytes,
            ExecuteTransactionRequestType::WaitForLocalExecution,
        )
        .await?;

    // Check StakedSui object
    let staked_sui: Vec<DelegatedStake> = http_client.get_delegated_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000, staked_sui[0].staked_sui.principal());
    assert_eq!(Some(20), staked_sui[0].staked_sui.sui_token_lock());

    assert!(matches!(
        staked_sui[0].delegation_status,
        DelegationStatus::Pending
    ));

    Ok(())
}
