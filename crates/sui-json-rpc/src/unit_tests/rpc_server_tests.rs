// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::{
    CoinReadApiClient, GovernanceReadApiClient, IndexerApiClient, ReadApiClient,
    TransactionBuilderClient, WriteApiClient,
};
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(not(msim))]
use std::str::FromStr;
use std::time::Duration;
use sui_config::genesis_config::DEFAULT_GAS_AMOUNT;
use sui_config::genesis_config::DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT;
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_json::{call_args, type_args};
use sui_json_rpc_types::ObjectChange;
use sui_json_rpc_types::ObjectsPage;
use sui_json_rpc_types::{
    Balance, CoinPage, DelegatedStake, StakeStatus, SuiCoinMetadata, SuiExecutionStatus,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions, TransactionBlockBytes,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_types::balance::Supply;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::coin::{TreasuryCap, COIN_MODULE_NAME};
use sui_types::digests::ObjectDigest;
use sui_types::gas_coin::GAS;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_struct_tag, SUI_FRAMEWORK_ADDRESS};
use test_utils::network::TestClusterBuilder;
use tokio::time::sleep;

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
        )
        .await?;
    assert_eq!(5, objects.data.len());

    // Multiget objectIDs test
    let object_digests = objects
        .data
        .iter()
        .map(|o| o.object().unwrap().object_id)
        .collect();

    let object_resp = http_client.multi_get_objects(object_digests, None).await?;
    assert_eq!(5, object_resp.len());
    Ok(())
}

#[tokio::test]
async fn test_get_package_with_display_should_not_fail() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let response = http_client
        .get_object(
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            Some(SuiObjectDataOptions::new().with_display()),
        )
        .await;
    assert!(response.is_ok());
    let response: SuiObjectResponse = response?;
    assert!(response
        .into_object()
        .unwrap()
        .display
        .unwrap()
        .data
        .is_none());
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
        )
        .await?
        .data;

    let obj = objects.clone().first().unwrap().object().unwrap().object_id;
    let gas = objects.clone().last().unwrap().object().unwrap().object_id;

    let transaction_bytes: TransactionBlockBytes = http_client
        .transfer_object(*address, obj, Some(gas), 10_000.into(), *address)
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();
    let tx_bytes1 = tx_bytes.clone();
    let dryrun_response = http_client.dry_run_transaction_block(tx_bytes).await?;

    let tx_response: SuiTransactionBlockResponse = http_client
        .execute_transaction_block(
            tx_bytes1,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_object_changes(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    assert_same_object_changes_ignoring_version_and_digest(
        dryrun_response.object_changes,
        tx_response.object_changes.unwrap(),
    );
    Ok(())
}

fn assert_same_object_changes_ignoring_version_and_digest(
    expected: Vec<ObjectChange>,
    actual: Vec<ObjectChange>,
) {
    fn collect_changes_mask_version_and_digest(
        changes: Vec<ObjectChange>,
    ) -> BTreeMap<ObjectID, ObjectChange> {
        changes
            .into_iter()
            .map(|mut change| {
                let object_id = change.object_id();
                // ignore the version and digest for comparison
                change.mask_for_test(SequenceNumber::MAX, ObjectDigest::MAX);
                (object_id, change)
            })
            .collect()
    }
    let expected = collect_changes_mask_version_and_digest(expected);
    let actual = collect_changes_mask_version_and_digest(actual);
    assert!(expected.keys().all(|id| actual.contains_key(id)));
    assert!(actual.keys().all(|id| expected.contains_key(id)));
    for (id, exp) in &expected {
        let act = actual.get(id).unwrap();
        assert_eq!(act, exp);
    }
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
        )
        .await?;
    let gas = objects.data.first().unwrap().object().unwrap();

    let compiled_package = BuildConfig::new_for_testing()
        .build(Path::new("../../sui_programmability/examples/fungible_tokens").to_path_buf())?;
    let compiled_modules_bytes =
        compiled_package.get_package_base64(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let transaction_bytes: TransactionBlockBytes = http_client
        .publish(
            *address,
            compiled_modules_bytes,
            dependencies,
            Some(gas.object_id),
            10000.into(),
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    matches!(tx_response, SuiTransactionBlockResponse {effects, ..} if effects.as_ref().unwrap().created().len() == 6);
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
        )
        .await?
        .data;

    let gas = objects.first().unwrap().object().unwrap();
    let coin = &objects[1].object()?;

    // now do the call
    let package_id = ObjectID::new(SUI_FRAMEWORK_ADDRESS.into_bytes());
    let module = "pay".to_string();
    let function = "split".to_string();

    let transaction_bytes: TransactionBlockBytes = http_client
        .move_call(
            *address,
            package_id,
            module,
            function,
            type_args![GAS::type_tag()]?,
            call_args!(coin.object_id, 10)?,
            Some(gas.object_id),
            10_000.into(),
            None,
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    matches!(tx_response, SuiTransactionBlockResponse {effects, ..} if effects.as_ref().unwrap().created().len() == 1);
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
        )
        .await?
        .data;

    for obj in objects {
        let oref = obj.into_object().unwrap();
        let result = http_client
            .get_object(
                oref.object_id,
                Some(SuiObjectDataOptions::new().with_owner()),
            )
            .await?;
        assert!(
            matches!(result, SuiObjectResponse { data: Some(object), .. } if oref.object_id == object.object_id && &object.owner.unwrap().get_owner_address()? == address)
        );
    }
    Ok(())
}

#[sim_test]
async fn test_get_object_data_with_content() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();
    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new().with_content().with_owner(),
            )),
            None,
            None,
        )
        .await?
        .data;

    for obj in objects {
        let oref = obj.into_object().unwrap();
        let result = http_client
            .get_object(
                oref.object_id,
                Some(SuiObjectDataOptions::new().with_content().with_owner()),
            )
            .await?;
        assert!(
            matches!(result, SuiObjectResponse { data: Some(object), .. } if oref.object_id == object.object_id && &object.owner.unwrap().get_owner_address()? == address)
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
    assert_eq!(2, result.data.len(), "{:?}", result);
    assert!(!result.has_next_page);

    let result: CoinPage = http_client
        .get_coins(
            *address,
            Some("0x2::sui::SUI".into()),
            result.next_cursor,
            None,
        )
        .await?;
    assert_eq!(0, result.data.len(), "{:?}", result);
    assert!(!result.has_next_page);

    Ok(())
}

#[sim_test]
async fn test_get_balance() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let result: Balance = http_client.get_balance(address, None).await?;
    assert_eq!("0x2::sui::SUI", result.coin_type);
    assert_eq!(
        (DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT as u64 * DEFAULT_GAS_AMOUNT) as u128,
        result.total_balance
    );
    assert_eq!(
        DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        result.coin_object_count
    );
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
        )
        .await?
        .data;

    let gas = objects.first().unwrap().object().unwrap();

    // Publish test coin package
    let compiled_package = BuildConfig::default()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?;
    let compiled_modules_bytes =
        compiled_package.get_package_base64(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let transaction_bytes: TransactionBlockBytes = http_client
        .publish(
            *address,
            compiled_modules_bytes,
            dependencies,
            Some(gas.object_id),
            100_000_000.into(),
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
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
        .await?
        .unwrap();

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
        )
        .await?
        .data;
    let gas = objects.first().unwrap().object().unwrap();

    // Publish test coin package
    let compiled_package = BuildConfig::new_for_testing()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?;
    let compiled_modules_bytes =
        compiled_package.get_package_base64(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let transaction_bytes: TransactionBlockBytes = http_client
        .publish(
            *address,
            compiled_modules_bytes,
            dependencies,
            Some(gas.object_id),
            100_000_000.into(),
        )
        .await?;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response: SuiTransactionBlockResponse = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
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

    let transaction_bytes: TransactionBlockBytes = http_client
        .move_call(
            *address,
            SUI_FRAMEWORK_ADDRESS.into(),
            COIN_MODULE_NAME.to_string(),
            "mint_and_transfer".into(),
            type_args![coin_name]?,
            call_args![treasury_cap, 100000, address]?,
            Some(gas.object_id),
            10_000_000.into(),
            None,
        )
        .await?;

    let tx = transaction_bytes.to_data()?;

    let tx = to_sender_signed_transaction(tx, keystore.get_key(address)?);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new().with_effects()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let SuiTransactionBlockResponse { effects, .. } = tx_response;

    assert_eq!(SuiExecutionStatus::Success, *effects.unwrap().status());

    let result: Supply = http_client.get_total_supply(coin_name.clone()).await?;
    assert_eq!(100000, result.value);

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
    let transaction_bytes: TransactionBlockBytes = http_client
        .request_add_stake(
            *address,
            vec![coin],
            Some(1000000000.into()),
            validator,
            None,
            100_000_000.into(),
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000000, staked_sui[0].stakes[0].principal);
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

#[ignore]
#[sim_test]
async fn test_unstaking() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .build()
        .await?;

    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let coins: CoinPage = http_client.get_coins(*address, None, None, None).await?;
    assert_eq!(5, coins.data.len());

    // Check StakedSui object before test
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert!(staked_sui.is_empty());

    let validator = http_client
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    // Delegate some SUI
    for i in 0..3 {
        let transaction_bytes: TransactionBlockBytes = http_client
            .request_add_stake(
                *address,
                vec![coins.data[i].coin_object_id],
                Some(1000000000.into()),
                validator,
                None,
                100_000_000.into(),
            )
            .await?;
        let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

        http_client
            .execute_transaction_block(
                tx_bytes,
                signatures,
                Some(SuiTransactionBlockResponseOptions::new()),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
    }
    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000000, staked_sui[0].stakes[0].principal);

    sleep(Duration::from_millis(10000)).await;

    let staked_sui_copy = http_client
        .get_stakes_by_ids(vec![
            staked_sui[0].stakes[0].staked_sui_id,
            staked_sui[0].stakes[1].staked_sui_id,
            staked_sui[0].stakes[2].staked_sui_id,
        ])
        .await?;

    assert!(matches!(
        &staked_sui_copy[0].stakes[0].status,
        StakeStatus::Active {
            estimated_reward: _
        }
    ));
    assert!(matches!(
        &staked_sui_copy[0].stakes[1].status,
        StakeStatus::Active {
            estimated_reward: _
        }
    ));
    assert!(matches!(
        &staked_sui_copy[0].stakes[2].status,
        StakeStatus::Active {
            estimated_reward: _
        }
    ));

    let transaction_bytes: TransactionBlockBytes = http_client
        .request_withdraw_stake(
            *address,
            staked_sui_copy[0].stakes[2].staked_sui_id,
            None,
            1_000_000.into(),
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    sleep(Duration::from_millis(20000)).await;

    let staked_sui_copy = http_client
        .get_stakes_by_ids(vec![
            staked_sui[0].stakes[0].staked_sui_id,
            staked_sui[0].stakes[1].staked_sui_id,
            staked_sui[0].stakes[2].staked_sui_id,
        ])
        .await?;

    assert!(matches!(
        &staked_sui_copy[0].stakes[0].status,
        StakeStatus::Active {
            estimated_reward: _
        }
    ));
    assert!(matches!(
        &staked_sui_copy[0].stakes[1].status,
        StakeStatus::Active {
            estimated_reward: _
        }
    ));
    assert!(matches!(
        &staked_sui_copy[0].stakes[2].status,
        StakeStatus::Unstaked
    ));
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
    let transaction_bytes: TransactionBlockBytes = http_client
        .request_add_stake(
            *address,
            vec![
                coins.data[0].coin_object_id,
                coins.data[1].coin_object_id,
                coins.data[2].coin_object_id,
            ],
            Some(1000000000.into()),
            validator,
            None,
            100_000_000.into(),
        )
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let dryrun_response = http_client
        .dry_run_transaction_block(tx_bytes.clone())
        .await?;

    let executed_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_balance_changes()
                    .with_input(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    // Check coin balance changes on dry run
    assert_eq!(
        dryrun_response.balance_changes,
        executed_response.balance_changes.unwrap()
    );

    // Check that inputs for dry run match the executed transaction
    assert_eq!(
        dryrun_response.input,
        executed_response.transaction.unwrap().data
    );

    // Check DelegatedStake object
    let staked_sui: Vec<DelegatedStake> = http_client.get_stakes(*address).await?;
    assert_eq!(1, staked_sui.len());
    assert_eq!(1000000000, staked_sui[0].stakes[0].principal);
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
    assert_eq!((genesis_coin_amount * 3) - 1000000000, new_coin.balance);

    Ok(())
}
