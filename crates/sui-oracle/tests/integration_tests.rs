// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::str::FromStr;

use shared_crypto::intent::Intent;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_move_build::BuildConfig;
use sui_rpc_api::Client;
use sui_sdk::sui_sdk_types::StructTag;
use sui_sdk::types::Identifier;
use sui_sdk::types::base_types::{ObjectID, SuiAddress};
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::transaction::{
    CallArg, ObjectArg, SharedObjectMutability, Transaction, TransactionData,
};
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GasCoin;
use sui_types::{TypeTag, parse_sui_type_tag};

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
                mutability: SharedObjectMutability::Mutable,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutability: SharedObjectMutability::Immutable,
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
        .await
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .execute_transaction_and_wait_for_checkpoint(&tx)
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
                mutability: SharedObjectMutability::Mutable,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutability: SharedObjectMutability::Immutable,
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
        .await
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .execute_transaction_and_wait_for_checkpoint(&tx)
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
                mutability: SharedObjectMutability::Mutable,
            }))
            .unwrap();

        let clock = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::from_str("0x6").unwrap(),
                initial_shared_version: 1.into(),
                mutability: SharedObjectMutability::Immutable,
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
            .await
            .unwrap();

        let tx = Transaction::from_data(data.clone(), vec![signature]);

        let result = client
            .execute_transaction_and_wait_for_checkpoint(&tx)
            .await
            .unwrap();

        assert!(result.effects.status().is_ok());
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
            mutability: SharedObjectMutability::Immutable,
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
                    mutability: SharedObjectMutability::Immutable,
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
        .await
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .execute_transaction_and_wait_for_checkpoint(&tx)
        .await
        .unwrap();

    assert!(result.effects.status().is_ok());
}

async fn get_gas(client: &Client, sender: SuiAddress) -> (ObjectRef, u64) {
    let gas = client
        .get_owned_objects(sender, Some(GasCoin::type_()), None, None)
        .await
        .unwrap();
    let gas = gas.items[0].compute_object_reference();
    let gas_price = client.get_reference_gas_price().await.unwrap();

    (gas, gas_price)
}

async fn init_test_client() -> (Client, Keystore, SuiAddress) {
    let client = Client::new("https://rpc.devnet.sui.io:443").unwrap();

    let keystore = Keystore::File(
        FileBasedKeystore::load_or_create(
            &dirs::home_dir()
                .unwrap()
                .join(".sui/sui_config/sui.keystore"),
        )
        .unwrap(),
    );
    let sender: SuiAddress = keystore.addresses()[0];
    let gas = client
        .get_owned_objects(sender, Some(GasCoin::type_()), None, None)
        .await
        .unwrap();

    assert!(
        !gas.items.is_empty(),
        "No gas coin found in account, please fund [{}]",
        sender
    );

    (client, keystore, sender)
}

async fn publish_package(
    sender: SuiAddress,
    keystore: &Keystore,
    client: &Client,
    path: &Path,
) -> ObjectID {
    let compiled_package = BuildConfig::new_for_testing().build(path).unwrap();
    let all_module_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();
    let gas = client
        .get_owned_objects(sender, Some(GasCoin::type_()), None, None)
        .await
        .unwrap();
    let gas = gas.items[0].compute_object_reference();
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
        .await
        .unwrap();

    let tx = Transaction::from_data(data.clone(), vec![signature]);

    let result = client
        .execute_transaction_and_wait_for_checkpoint(&tx)
        .await
        .unwrap();
    assert!(result.effects.status().is_ok(),);

    result.get_new_package_obj().unwrap().0
}

async fn create_oracle(
    sender: SuiAddress,
    keystore: &Keystore,
    client: &Client,
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
        .get_owned_objects(sender, Some(GasCoin::type_()), None, None)
        .await
        .unwrap();
    let gas = gas.items[0].compute_object_reference();
    let gas_price = client.get_reference_gas_price().await.unwrap();
    let data = TransactionData::new_programmable(sender, vec![gas], pt, 1000000000, gas_price);

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .await
        .unwrap();
    let tx = Transaction::from_data(data.clone(), vec![signature]);
    let result = client
        .execute_transaction_and_wait_for_checkpoint(&tx)
        .await
        .unwrap();
    assert!(result.effects.status().is_ok(),);

    let simple_oracle = result
        .changed_objects
        .iter()
        .find(|change| {
            let Ok(ty) = change.object_type().parse::<StructTag>() else {
                return false;
            };
            ty.name().as_str() == "SimpleOracle"
        })
        .unwrap();

    (
        simple_oracle.object_id().parse().unwrap(),
        simple_oracle.output_version().into(),
    )
}
