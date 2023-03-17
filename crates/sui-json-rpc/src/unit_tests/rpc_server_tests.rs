// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
#[cfg(not(msim))]
use std::str::FromStr;

use sui_config::SUI_KEYSTORE_FILENAME;
use sui_framework_build::compiled_package::BuildConfig;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::ObjectChange;
use sui_json_rpc_types::ObjectsPage;
use sui_json_rpc_types::SuiTransactionResponseQuery;
use sui_json_rpc_types::{
    Balance, CoinPage, DelegatedStake, StakeStatus, SuiCoinMetadata, SuiExecutionStatus,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiTransactionEffectsAPI,
    SuiTransactionResponse, SuiTransactionResponseOptions, TransactionBytes,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_macros::sim_test;
use sui_types::balance::Supply;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::coin::{TreasuryCap, COIN_MODULE_NAME, LOCKED_COIN_MODULE_NAME};
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::query::TransactionFilter;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_struct_tag, parse_sui_type_tag, SUI_FRAMEWORK_ADDRESS};
use test_utils::network::TestClusterBuilder;

use crate::api::{
    CoinReadApiClient, GovernanceReadApiClient, ReadApiClient, TransactionBuilderClient,
    WriteApiClient,
};

#[sim_test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
            None,
        )
        .await?;
    assert_eq!(5, objects.data.len());

    // Multiget objectIDs test
    let object_digests = objects
        .data
        .iter()
        .map(|o| o.object().unwrap().object_id)
        .collect();

    let object_resp = http_client
        .multi_get_object_with_options(object_digests, None)
        .await?;
    assert_eq!(5, object_resp.len());
    Ok(())
}

#[sim_test]
async fn test_public_transfer_object() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;

    let obj = objects.clone().first().unwrap().object().unwrap().object_id;
    let gas = objects.clone().last().unwrap().object().unwrap().object_id;

    let transaction_bytes: TransactionBytes = http_client
        .transfer_object(*address, obj, Some(gas), 1000, *address)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();
    let tx_bytes1 = tx_bytes.clone();
    let dryrun_response = http_client.dry_run_transaction(tx_bytes).await?;

    let tx_response: SuiTransactionResponse = http_client
        .execute_transaction(
            tx_bytes1,
            signatures,
            Some(SuiTransactionResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let SuiTransactionResponse { effects, .. } = tx_response;
    assert_eq!(
        dryrun_response.effects.transaction_digest(),
        effects.unwrap().transaction_digest()
    );
    Ok(())
}

#[sim_test]
async fn test_publish() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?;
    let gas = objects.data.first().unwrap().object().unwrap();

    let compiled_modules = BuildConfig::new_for_testing()
        .build(Path::new("../../sui_programmability/examples/fungible_tokens").to_path_buf())?
        .get_package_base64(/* with_unpublished_deps */ false);

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    matches!(tx_response, SuiTransactionResponse {effects, ..} if effects.as_ref().unwrap().created().len() == 6);
    Ok(())
}

#[sim_test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;

    let gas = objects.first().unwrap().object().unwrap();
    let coin = &objects[1].object()?;

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

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    matches!(tx_response, SuiTransactionResponse {effects, ..} if effects.as_ref().unwrap().created().len() == 1);
    Ok(())
}

#[sim_test]
async fn test_get_object_info() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();
    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;

    for obj in objects {
        let oref = obj.into_object().unwrap();
        let result = http_client
            .get_object_with_options(
                oref.object_id,
                Some(SuiObjectDataOptions::new().with_owner()),
            )
            .await?;
        assert!(
            matches!(result, SuiObjectResponse::Exists(object) if oref.object_id == object.object_id && &object.owner.unwrap().get_owner_address()? == address)
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
    assert!(!result.has_next_page);

    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::TestCoin".into()), None, None)
        .await?;
    assert_eq!(0, result.data.len());

    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::SUI".into()), None, None)
        .await?;
    assert_eq!(5, result.data.len());
    assert!(!result.has_next_page);

    // Test paging
    let result: CoinPage = http_client
        .get_coins(*address, Some("0x2::sui::SUI".into()), None, Some(3))
        .await?;
    assert_eq!(3, result.data.len());
    assert!(result.has_next_page);

    let result: CoinPage = http_client
        .get_coins(
            *address,
            Some("0x2::sui::SUI".into()),
            result.next_cursor,
            Some(3),
        )
        .await?;
    assert_eq!(2, result.data.len());
    assert!(!result.has_next_page);

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

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;

    let gas = objects.first().unwrap().object().unwrap();

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
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionResponseOptions::new()
                    .with_object_changes()
                    .with_events(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let object_changes = tx_response.object_changes.unwrap();
    let package_id = object_changes
        .iter()
        .find_map(|e| {
            if let ObjectChange::Published { package_id, .. } = e {
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

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;
    let gas = objects.first().unwrap().object().unwrap();

    // Publish test coin package
    let compiled_modules = BuildConfig::new_for_testing()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?
        .get_package_base64(/* with_unpublished_deps */ false);

    let transaction_bytes: TransactionBytes = http_client
        .publish(*address, compiled_modules, Some(gas.object_id), 10000)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response: SuiTransactionResponse = http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionResponseOptions::new()
                    .with_object_changes()
                    .with_events(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let object_changes = tx_response.object_changes.as_ref().unwrap();
    let package_id = object_changes
        .iter()
        .find_map(|e| {
            if let ObjectChange::Published { package_id, .. } = e {
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

    let object_changes = tx_response.object_changes.as_ref().unwrap();
    let treasury_cap = object_changes
        .iter()
        .find_map(|e| {
            if let ObjectChange::Created {
                object_id,
                object_type,
                ..
            } = e
            {
                if &TreasuryCap::type_(parse_sui_struct_tag(&coin_name).unwrap()) == object_type {
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
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let SuiTransactionResponse { effects, .. } = tx_response;

    assert_eq!(SuiExecutionStatus::Success, *effects.unwrap().status());

    let result: Supply = http_client.get_total_supply(coin_name.clone()).await?;
    assert_eq!(100000, result.value);

    Ok(())
}

#[sim_test]
async fn test_get_transaction() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;
    let gas_id = objects.last().unwrap().object().unwrap().object_id;

    // Make some transactions
    let mut tx_responses: Vec<SuiTransactionResponse> = Vec::new();
    for obj in &objects[..objects.len() - 1] {
        let oref = obj.object().unwrap();
        let transaction_bytes: TransactionBytes = http_client
            .transfer_object(*address, oref.object_id, Some(gas_id), 1000, *address)
            .await?;
        let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

        let response = http_client
            .execute_transaction(
                tx_bytes,
                signatures,
                Some(SuiTransactionResponseOptions::new()),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        tx_responses.push(response);
    }
    // test get_transactions_in_range_deprecated
    let tx: Vec<TransactionDigest> = http_client
        .get_transactions_in_range_deprecated(0, 10)
        .await?;
    assert_eq!(5, tx.len());

    // test get_transaction_batch
    let batch_responses: Vec<SuiTransactionResponse> = http_client
        .multi_get_transactions_with_options(tx, Some(SuiTransactionResponseOptions::new()))
        .await?;

    assert_eq!(5, batch_responses.len());

    for r in batch_responses.iter().skip(1) {
        assert!(tx_responses
            .iter()
            .any(|resp| matches!(resp, SuiTransactionResponse {digest, ..} if *digest == r.digest)))
    }

    // test get_transactions_in_range_deprecated with smaller range
    let tx: Vec<TransactionDigest> = http_client
        .get_transactions_in_range_deprecated(1, 3)
        .await?;
    assert_eq!(2, tx.len());

    // test get_transaction
    for tx_digest in tx {
        let response: SuiTransactionResponse = http_client
            .get_transaction_with_options(tx_digest, Some(SuiTransactionResponseOptions::new()))
            .await?;
        assert!(tx_responses.iter().any(
            |resp| matches!(resp, SuiTransactionResponse {digest, ..} if *digest == response.digest)
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
    let mut tx_responses: Vec<SuiTransactionResponse> = Vec::new();

    let client = context.get_client().await.unwrap();

    for address in cluster.accounts.iter() {
        let objects = client
            .read_api()
            .get_owned_objects(
                *address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::new()
                        .with_type()
                        .with_owner()
                        .with_previous_transaction(),
                )),
                None,
                None,
                None,
            )
            .await?
            .data;
        let gas_id = objects.last().unwrap().object().unwrap().object_id;

        // Make some transactions
        for obj in &objects[..objects.len() - 1] {
            let oref = obj.object().unwrap();
            let data = client
                .transaction_builder()
                .transfer_object(*address, oref.object_id, Some(gas_id), 1000, *address)
                .await?;
            let tx = to_sender_signed_transaction(data, keystore.get_key(address).unwrap());

            let response = client
                .quorum_driver()
                .execute_transaction(
                    tx,
                    SuiTransactionResponseOptions::new(),
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
        .query_transactions(SuiTransactionResponseQuery::default(), None, Some(3), true)
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());
    assert!(tx.has_next_page);

    // test get all transactions paged
    let first_page = client
        .read_api()
        .query_transactions(SuiTransactionResponseQuery::default(), None, Some(5), false)
        .await
        .unwrap();
    assert_eq!(5, first_page.data.len());
    assert!(first_page.has_next_page);

    let second_page = client
        .read_api()
        .query_transactions(
            SuiTransactionResponseQuery::default(),
            first_page.next_cursor,
            None,
            false,
        )
        .await
        .unwrap();
    assert_eq!(16, second_page.data.len());
    assert!(!second_page.has_next_page);

    let mut all_txs_rev = first_page.data.clone();
    all_txs_rev.extend(second_page.data);
    all_txs_rev.reverse();

    // test get 10 latest transactions paged
    let latest = client
        .read_api()
        .query_transactions(SuiTransactionResponseQuery::default(), None, Some(10), true)
        .await
        .unwrap();
    assert_eq!(10, latest.data.len());
    assert_eq!(Some(all_txs_rev[9].digest), latest.next_cursor);
    assert_eq!(all_txs_rev[0..10], latest.data);
    assert!(latest.has_next_page);

    // test get from address txs in ascending order
    let address_txs_asc = client
        .read_api()
        .query_transactions(
            SuiTransactionResponseQuery::new_with_filter(TransactionFilter::FromAddress(
                cluster.accounts[0],
            )),
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
        .query_transactions(
            SuiTransactionResponseQuery::new_with_filter(TransactionFilter::FromAddress(
                cluster.accounts[0],
            )),
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
        .query_transactions(SuiTransactionResponseQuery::default(), None, Some(20), true)
        .await
        .unwrap();
    assert_eq!(20, tx.data.len());

    // test get_transaction
    for tx_resp in tx.data {
        let response: SuiTransactionResponse = client
            .read_api()
            .get_transaction_with_options(tx_resp.digest, SuiTransactionResponseOptions::new())
            .await
            .unwrap();
        assert!(tx_responses
            .iter()
            .any(|resp| resp.digest == response.digest))
    }

    Ok(())
}

#[sim_test]
async fn test_locked_sui() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?
        .data;
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
            2000,
            None,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
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
async fn test_staking() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects: ObjectsPage = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
            None,
        )
        .await?;
    assert_eq!(5, objects.data.len());

    // Check StakedSui object before test
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert!(staked_sui.is_empty());

    let validator = http_client
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    let coin = objects.data[0].object()?.object_id;
    // Delegate some SUI
    let transaction_bytes: TransactionBytes = http_client
        .request_add_stake(*address, vec![coin], Some(1000000), validator, None, 10000)
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000, staked_sui[0].stakes[0].principal);
    assert!(matches!(
        staked_sui[0].stakes[0].status,
        StakeStatus::Pending
    ));
    let staked_sui_copy = http_client
        .get_stakes_by_ids(vec![staked_sui[0].stakes[0].staked_sui_id])
        .await?;
    assert_eq!(
        staked_sui[0].stakes[0].staked_sui_id,
        staked_sui_copy[0].stakes[0].staked_sui_id
    );
    Ok(())
}

#[sim_test]
async fn test_staking_multiple_coins() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    assert_eq!(5, coins.data.len());

    let genesis_coin_amount = coins.data[0].balance;

    // Check StakedSui object before test
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert!(staked_sui.is_empty());

    let validator = http_client
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;
    // Delegate some SUI
    let transaction_bytes: TransactionBytes = http_client
        .request_add_stake(
            *address,
            vec![
                coins.data[0].coin_object_id,
                coins.data[1].coin_object_id,
                coins.data[2].coin_object_id,
            ],
            Some(1000000),
            validator,
            None,
            10000,
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    http_client
        .execute_transaction(
            tx_bytes,
            signatures,
            Some(SuiTransactionResponseOptions::new()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000, staked_sui[0].stakes[0].principal);
    assert!(matches!(
        staked_sui[0].stakes[0].status,
        StakeStatus::Pending
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
