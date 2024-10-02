// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::str::FromStr;

use shared_crypto::intent::Intent;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::{ObjectChange, SuiExecutionStatus};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_move_build::BuildConfig;
use sui_sdk::rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::types::base_types::{ObjectID, SuiAddress};
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_sdk::types::transaction::{CallArg, ObjectArg, Transaction, TransactionData};
use sui_sdk::types::Identifier;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::{parse_sui_type_tag, TypeTag};

// Integration tests for SUI Oracle, these test can be run manually on local or remote testnet.
#[ignore]
#[tokio::test]
async fn test_publish_primitive() {
    let (client, keystore, sender) = init_test_client().await;
    // publish package if not exists
    let package = option_env!("package_id")
        .map(|s| ObjectID::from_str(s).unwrap())
        .unwrap_or(publish_package(sender, &keystore, &client, Path::new("move/oracle")).await);
    let module = Identifier::from_str("simple_oracle").unwrap();

    // create simple oracle if not exists
    let (simple_oracle_id, version) = option_env!("oracle_id")
        .and_then(|id| {
            option_env!("oracle_version").map(|version| {
                (
                    ObjectID::from_str(id).unwrap(),
                    u64::from_str(version).unwrap().into(),
                )
            })
        })
        .unwrap_or(create_oracle(sender, &keystore, &client, package, module.clone()).await);

    // publish oracle data
    let submit_data = Identifier::from_str("submit_data").unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();

    for i in 1..200 {
        let ticker = format!("SUI {}", i);

        let value = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&rand::random::<u64>()).unwrap(),
            ))
            .unwrap();

        let simple_oracle = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: simple_oracle_id,
                initial_shared_version: version,
                mutable: true,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutable: false,
            }))
            .unwrap();

        let ticker = builder
            .input(CallArg::Pure(bcs::to_bytes(ticker.as_bytes()).unwrap()))
            .unwrap();
        let identifier = builder
            .input(CallArg::Pure(
                bcs::to_bytes("identifier".as_bytes()).unwrap(),
            ))
            .unwrap();

        builder.programmable_move_call(
            package,
            module.clone(),
            submit_data.clone(),
            vec![TypeTag::U64],
            vec![simple_oracle, clock, ticker, value, identifier],
        );
    }

    let pt = builder.finish();
    let (gas, gas_price) = get_gas(&client, sender).await;
    let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    println!("{:#?}", result)
}

#[ignore]
#[tokio::test]
async fn test_publish_complex_value() {
    let (client, keystore, sender) = init_test_client().await;
    // publish package if not exists
    let package = option_env!("package_id")
        .map(|s| ObjectID::from_str(s).unwrap())
        .unwrap_or(publish_package(sender, &keystore, &client, Path::new("move/oracle")).await);
    let module = Identifier::from_str("simple_oracle").unwrap();

    // create simple oracle if not exists
    let (simple_oracle_id, version) = option_env!("oracle_id")
        .and_then(|id| {
            option_env!("oracle_version").map(|version| {
                (
                    ObjectID::from_str(id).unwrap(),
                    u64::from_str(version).unwrap().into(),
                )
            })
        })
        .unwrap_or(create_oracle(sender, &keystore, &client, package, module.clone()).await);

    // publish oracle data
    let submit_data = Identifier::from_str("submit_data").unwrap();
    let data_types = Identifier::from_str("decimal_value").unwrap();
    let create_decimal = Identifier::from_str("new").unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();

    let decimal = builder
        .input(CallArg::Pure(bcs::to_bytes(&6u8).unwrap()))
        .unwrap();

    for i in 1..200 {
        let ticker = format!("SUI {}", i);

        let value = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&rand::random::<u64>()).unwrap(),
            ))
            .unwrap();

        let decimal_value = builder.programmable_move_call(
            package,
            data_types.clone(),
            create_decimal.clone(),
            vec![],
            vec![value, decimal],
        );

        let simple_oracle = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: simple_oracle_id,
                initial_shared_version: version,
                mutable: true,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutable: false,
            }))
            .unwrap();

        let ticker = builder
            .input(CallArg::Pure(bcs::to_bytes(ticker.as_bytes()).unwrap()))
            .unwrap();
        let identifier = builder
            .input(CallArg::Pure(
                bcs::to_bytes("identifier".as_bytes()).unwrap(),
            ))
            .unwrap();

        builder.programmable_move_call(
            package,
            module.clone(),
            submit_data.clone(),
            vec![parse_sui_type_tag(&format!("{package}::decimal_value::DecimalValue")).unwrap()],
            vec![simple_oracle, clock, ticker, decimal_value, identifier],
        );
    }

    let pt = builder.finish();
    let (gas, gas_price) = get_gas(&client, sender).await;
    let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    println!("{:#?}", result)
}

#[ignore]
#[tokio::test]
async fn test_consume_oracle_data() {
    let (client, keystore, sender) = init_test_client().await;
    // publish package if not exists
    let Some(package) = option_env!("package_id").map(|s| ObjectID::from_str(s).unwrap()) else {
        panic!("package_id not set");
    };

    let module = Identifier::from_str("simple_oracle").unwrap();

    // create simple oracle
    let mut oracles = vec![];
    for _ in 0..3 {
        let (simple_oracle_id, version) =
            create_oracle(sender, &keystore, &client, package, module.clone()).await;
        oracles.push((simple_oracle_id, version));

        // publish oracle data
        let submit_data = Identifier::from_str("submit_data").unwrap();
        let data_types = Identifier::from_str("decimal_value").unwrap();
        let create_decimal = Identifier::from_str("new").unwrap();
        let mut builder = ProgrammableTransactionBuilder::new();

        // Create decimal value
        let decimal = builder
            .input(CallArg::Pure(bcs::to_bytes(&6u8).unwrap()))
            .unwrap();

        let value = builder
            .input(CallArg::Pure(bcs::to_bytes(&10000000u64).unwrap()))
            .unwrap();

        let decimal_value = builder.programmable_move_call(
            package,
            data_types.clone(),
            create_decimal.clone(),
            vec![],
            vec![value, decimal],
        );
        // publish data
        let simple_oracle = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: simple_oracle_id,
                initial_shared_version: version,
                mutable: true,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutable: false,
            }))
            .unwrap();

        let ticker = builder
            .input(CallArg::Pure(bcs::to_bytes("SUIUSD".as_bytes()).unwrap()))
            .unwrap();
        let identifier = builder
            .input(CallArg::Pure(
                bcs::to_bytes("identifier".as_bytes()).unwrap(),
            ))
            .unwrap();

        builder.programmable_move_call(
            package,
            module.clone(),
            submit_data.clone(),
            vec![parse_sui_type_tag(&format!("{package}::decimal_value::DecimalValue")).unwrap()],
            vec![simple_oracle, clock, ticker, decimal_value, identifier],
        );

        let pt = builder.finish();
        let (gas, gas_price) = get_gas(&client, sender).await;
        let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

        let signature = keystore
            .sign_secure(&sender, &data, Intent::sui_transaction())
            .unwrap();

        let tx = Transaction::from_data(data.clone(), vec![signature]);

        let result = client
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await
            .unwrap();

        assert!(result.effects.unwrap().status().is_ok());
    }

    let (simple_oracle_id, version) = *oracles.first().unwrap();

    // publish test package
    let test_package =
        publish_package(sender, &keystore, &client, Path::new("tests/data/Test")).await;
    // get data
    let mut builder = ProgrammableTransactionBuilder::new();
    let simple_oracle = builder
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: simple_oracle_id,
            initial_shared_version: version,
            mutable: false,
        }))
        .unwrap();
    let ticker = builder
        .input(CallArg::Pure(bcs::to_bytes("SUIUSD".as_bytes()).unwrap()))
        .unwrap();
    let data = builder.programmable_move_call(
        package,
        module,
        Identifier::from_str("get_latest_data").unwrap(),
        vec![parse_sui_type_tag(&format!("{package}::decimal_value::DecimalValue")).unwrap()],
        vec![simple_oracle, ticker],
    );

    // call simple_fx_ptb
    let test_module = Identifier::from_str("test_module").unwrap();
    let simple_fx_ptb = Identifier::from_str("simple_fx_ptb").unwrap();
    let mist_amount = builder
        .input(CallArg::Pure(bcs::to_bytes(&10000000u64).unwrap()))
        .unwrap();
    builder.programmable_move_call(
        test_package,
        test_module.clone(),
        simple_fx_ptb,
        vec![],
        vec![data, mist_amount],
    );

    // call simple_fx
    let simple_fx = Identifier::from_str("simple_fx").unwrap();
    let mist_amount = builder
        .input(CallArg::Pure(bcs::to_bytes(&10000000u64).unwrap()))
        .unwrap();

    builder.programmable_move_call(
        test_package,
        test_module.clone(),
        simple_fx,
        vec![],
        vec![simple_oracle, mist_amount],
    );

    // Call trusted_fx
    let trusted_fx = Identifier::from_str("trusted_fx").unwrap();
    let oracles = oracles
        .into_iter()
        .map(|(id, version)| {
            builder
                .input(CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version: version,
                    mutable: false,
                }))
                .unwrap()
        })
        .collect::<Vec<_>>();

    builder.programmable_move_call(
        test_package,
        test_module,
        trusted_fx,
        vec![],
        vec![oracles[0], oracles[1], oracles[2], mist_amount],
    );

    let pt = builder.finish();
    let (gas, gas_price) = get_gas(&client, sender).await;
    let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();

    assert!(result.effects.unwrap().status().is_ok());
}

async fn get_gas(client: &SuiClient, sender: SuiAddress) -> (ObjectRef, u64) {
    let gas = client
        .coin_read_api()
        .get_coins(sender, None, None, Some(1))
        .await
        .unwrap();
    let gas = gas.data[0].object_ref();
    let gas_price = client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    (gas, gas_price)
}

async fn init_test_client() -> (SuiClient, Keystore, SuiAddress) {
    let client = SuiClientBuilder::default()
        .build("https://rpc.devnet.sui.io:443")
        .await
        .unwrap();

    let keystore = Keystore::File(
        FileBasedKeystore::new(
            &dirs::home_dir()
                .unwrap()
                .join(".sui/sui_config/sui.keystore"),
        )
        .unwrap(),
    );
    let sender: SuiAddress = keystore.addresses()[0];
    let gas = client
        .coin_read_api()
        .get_coins(sender, None, None, Some(1))
        .await
        .unwrap();

    assert!(
        !gas.data.is_empty(),
        "No gas coin found in account, please fund [{}]",
        sender
    );

    (client, keystore, sender)
}

async fn publish_package(
    sender: SuiAddress,
    keystore: &Keystore,
    client: &SuiClient,
    path: &Path,
) -> ObjectID {
    let compiled_package = BuildConfig::new_for_testing().build(path).unwrap();
    let all_module_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();
    let gas = client
        .coin_read_api()
        .get_coins(sender, None, None, Some(1))
        .await
        .unwrap();
    let gas = gas.data[0].object_ref();
    let data = TransactionData::new_module(
        sender,
        gas,
        all_module_bytes,
        dependencies,
        1000000000,
        1000,
    );
    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    assert_eq!(
        &SuiExecutionStatus::Success,
        result.effects.unwrap().status()
    );

    let publish = result
        .object_changes
        .unwrap()
        .iter()
        .find(|change| matches!(change, ObjectChange::Published { .. }))
        .unwrap()
        .clone();

    let ObjectChange::Published { package_id, .. } = publish else {
        panic!("Expected published object change")
    };
    package_id
}

async fn create_oracle(
    sender: SuiAddress,
    keystore: &Keystore,
    client: &SuiClient,
    package: ObjectID,
    module: Identifier,
) -> (ObjectID, SequenceNumber) {
    let mut builder = ProgrammableTransactionBuilder::new();
    let create = Identifier::from_str("create").unwrap();
    builder
        .move_call(
            package,
            module,
            create,
            vec![],
            vec![
                CallArg::Pure(bcs::to_bytes("Teat Name".as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes("Test URL".as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes("Test description".as_bytes()).unwrap()),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let gas = client
        .coin_read_api()
        .get_coins(sender, None, None, Some(1))
        .await
        .unwrap();
    let gas = gas.data[0].object_ref();
    let gas_price = client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();
    let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();
    let tx = Transaction::from_data(data.clone(), vec![signature]);
    let result = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    assert_eq!(
        &SuiExecutionStatus::Success,
        result.effects.unwrap().status()
    );
    let simple_oracle = result.object_changes.unwrap().iter().find(|change| matches!(change, ObjectChange::Created {object_type,..} if object_type.name.as_str() == "SimpleOracle")).unwrap().clone();
    let ObjectChange::Created {
        object_id: simple_oracle_id,
        version,
        ..
    } = simple_oracle
    else {
        panic!("Expected created object change")
    };

    (simple_oracle_id, version)
}
