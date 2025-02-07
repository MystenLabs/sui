// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::io::Read;
use std::net::SocketAddr;
use std::os::unix::prelude::FileExt;
use std::{fmt::Write, fs::read_dir, path::PathBuf, str, thread, time::Duration};

use std::env;
#[cfg(not(msim))]
use std::str::FromStr;

use expect_test::expect;
use move_package::{lock_file::schema::ManagedPackage, BuildConfig as MoveBuildConfig};
use serde_json::json;
use sui::client_ptb::ptb::PTB;
use sui::key_identity::{get_identity_address, KeyIdentity};
use sui::sui_commands::IndexerArgs;
use sui_sdk::SuiClient;
use sui_test_transaction_builder::batch_make_transfer_transactions;
use sui_types::object::Owner;
use sui_types::transaction::{
    TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
    TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN,
    TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use tokio::time::sleep;

use sui::{
    client_commands::{
        estimate_gas_budget, Opts, OptsWithGas, SuiClientCommandResult, SuiClientCommands,
        SwitchResponse,
    },
    sui_commands::{parse_host_port, SuiCommand},
};
use sui_config::{
    PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG, SUI_GENESIS_FILENAME,
    SUI_KEYSTORE_ALIASES_FILENAME, SUI_KEYSTORE_FILENAME, SUI_NETWORK_CONFIG,
};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    get_new_package_obj_from_response, OwnedObjectRef, SuiExecutionStatus, SuiObjectData,
    SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
    SuiTransactionBlockDataAPI, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::sui_client_config::SuiClientConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_swarm_config::genesis_config::{AccountConfig, GenesisConfig};
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    Ed25519SuiSignature, Secp256k1SuiSignature, SignatureScheme, SuiKeyPair, SuiSignatureInner,
};
use sui_types::error::SuiObjectResponseError;
use sui_types::{base_types::ObjectID, crypto::get_key_pair, gas_coin::GasCoin};
use test_cluster::{TestCluster, TestClusterBuilder};

const TEST_DATA_DIR: &str = "tests/data/";

#[sim_test]
async fn test_genesis() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    let config = working_dir.join(SUI_NETWORK_CONFIG);

    // Start network without authorities
    let start = SuiCommand::Start {
        data_ingestion_dir: None,
        config_dir: Some(config),
        force_regenesis: false,
        with_faucet: None,
        fullnode_rpc_port: 9000,
        epoch_duration_ms: None,
        no_full_node: false,
        committee_size: None,
        indexer_feature_args: IndexerArgs::for_testing(),
    }
    .execute()
    .await;
    assert!(matches!(start, Err(..)));
    // Genesis
    SuiCommand::Genesis {
        working_dir: Some(working_dir.to_path_buf()),
        write_config: None,
        force: false,
        from_config: None,
        epoch_duration_ms: None,
        benchmark_ips: None,
        with_faucet: false,
        committee_size: None,
    }
    .execute()
    .await?;

    // Get all the new file names
    let files = read_dir(working_dir)?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(10, files.len());
    assert!(files.contains(&SUI_CLIENT_CONFIG.to_string()));
    assert!(files.contains(&SUI_NETWORK_CONFIG.to_string()));
    assert!(files.contains(&SUI_FULLNODE_CONFIG.to_string()));
    assert!(files.contains(&SUI_GENESIS_FILENAME.to_string()));
    assert!(files.contains(&SUI_KEYSTORE_FILENAME.to_string()));
    assert!(files.contains(&SUI_KEYSTORE_ALIASES_FILENAME.to_string()));

    // Check network config
    let network_conf =
        PersistedConfig::<NetworkConfig>::read(&working_dir.join(SUI_NETWORK_CONFIG))?;
    assert_eq!(4, network_conf.validator_configs().len());

    // Check wallet config
    let wallet_conf =
        PersistedConfig::<SuiClientConfig>::read(&working_dir.join(SUI_CLIENT_CONFIG))?;

    assert!(!wallet_conf.envs.is_empty());

    assert_eq!(5, wallet_conf.keystore.addresses().len());

    // Genesis 2nd time should fail
    let result = SuiCommand::Genesis {
        working_dir: Some(working_dir.to_path_buf()),
        write_config: None,
        force: false,
        from_config: None,
        epoch_duration_ms: None,
        benchmark_ips: None,
        with_faucet: false,
        committee_size: None,
    }
    .execute()
    .await;
    assert!(matches!(result, Err(..)));

    temp_dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut context = test_cluster.wallet;

    // Add 3 accounts
    for _ in 0..3 {
        context
            .config
            .keystore
            .add_key(None, SuiKeyPair::Ed25519(get_key_pair().1))?;
    }

    // Print all addresses
    SuiClientCommands::Addresses {
        sort_by_alias: true,
    }
    .execute(&mut context)
    .await
    .unwrap()
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context
        .config
        .keystore
        .get_alias_by_address(&address)
        .unwrap();
    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);
    // Print objects owned by `address`, passing its alias
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Alias(alias)),
    }
    .execute(context)
    .await?
    .print(true);
    let client = context.get_client().await?;
    let _object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    Ok(())
}

#[sim_test]
async fn test_ptb_publish_and_complex_arg_resolution() -> Result<(), anyhow::Error> {
    // Publish the package
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("ptb_complex_args_test_functions");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path: package_path.clone(),
        build_config,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();
    let package_id_str = package.reference.object_id.to_string();

    let start_call_result = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "test_module".to_string(),
        function: "new_shared".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await?;

    let shared_id_str =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            response.effects.unwrap().created().to_vec()[0]
                .reference
                .object_id
                .to_string()
        } else {
            unreachable!("Invalid response");
        };

    let complex_ptb_string = format!(
        r#"
         --assign p @{package_id_str}
         --assign s @{shared_id_str}
         # Use the shared object by immutable reference first
         --move-call "p::test_module::use_immut" s
         # Now use mutably -- we need to update the mutability of the object
         --move-call "p::test_module::use_mut" s
         # Make sure we handle different more complex pure arguments
         --move-call "p::test_module::use_ascii_string" "'foo bar baz'"
         --move-call "p::test_module::use_utf8_string" "'foo †††˚˚¬¬'"
         --gas-budget 100000000
        "#
    );

    let args = shlex::split(&complex_ptb_string).unwrap();
    sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await?;

    let delete_object_ptb_string = format!(
        r#"
         --assign p @{package_id_str}
         --assign s @{shared_id_str}
         # Use the shared object by immutable reference first
         --move-call "p::test_module::use_immut" s
         --move-call "p::test_module::delete_shared_object" s
         --gas-budget 100000000
        "#
    );

    let args = shlex::split(&delete_object_ptb_string).unwrap();
    sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await?;

    Ok(())
}

#[sim_test]
async fn test_ptb_publish() -> Result<(), anyhow::Error> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("ptb_complex_args_test_functions");

    let publish_ptb_string = format!(
        r#"
         --move-call sui::tx_context::sender
         --assign sender
         --publish {}
         --assign upgrade_cap
         --transfer-objects "[upgrade_cap]" sender
        "#,
        package_path.display()
    );
    let args = shlex::split(&publish_ptb_string).unwrap();
    sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await?;
    Ok(())
}

#[sim_test]
async fn test_custom_genesis() -> Result<(), anyhow::Error> {
    // Create and save genesis config file
    // Create 4 authorities, 1 account with 1 gas object with custom id

    let mut config = GenesisConfig::for_local_testing();
    config.accounts.clear();
    config.accounts.push(AccountConfig {
        address: None,
        gas_amounts: vec![500],
    });
    let mut cluster = TestClusterBuilder::new()
        .set_genesis_config(config)
        .build()
        .await;
    let address = cluster.get_address_0();
    let context = cluster.wallet_mut();

    assert_eq!(1, context.config.keystore.addresses().len());

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().object().unwrap().object_id;

    SuiClientCommands::Object {
        id: object_id,
        bcs: false,
    }
    .execute(context)
    .await?
    .print(true);

    SuiClientCommands::Object {
        id: object_id,
        bcs: true,
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context
        .config
        .keystore
        .get_alias_by_address(&address)
        .unwrap();

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?;

    let object_id = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_to_send = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    SuiClientCommands::Gas {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send an object
    SuiClientCommands::Transfer {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        object_id: object_to_send,
        opts: OptsWithGas::for_testing(Some(object_id), rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    // Fetch gas again, and use the alias instead of the address
    SuiClientCommands::Gas {
        address: Some(KeyIdentity::Alias(alias)),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address1 = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let address2 = SuiAddress::random_for_testing_only();

    let client = context.get_client().await?;
    // publish the object basics package
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("move_call_args_linter");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let package = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert!(
            response.status_ok().unwrap(),
            "Command failed: {:?}",
            response
        );
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .unwrap()
            .created()
            .iter()
            .find(
                |OwnedObjectRef {
                     owner,
                     reference: _,
                 }| matches!(owner, Owner::Immutable),
            )
            .unwrap()
            .reference
            .object_id
    } else {
        unreachable!("Invalid response");
    };

    // Print objects owned by `address1`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address1)),
    }
    .execute(context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
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

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let coins: Vec<_> = object_refs
        .iter()
        .filter(|object_ref| object_ref.object().unwrap().is_gas_coin())
        .collect();
    let gas = coins.first().unwrap().object()?.object_id;
    let obj = coins.get(1).unwrap().object()?.object_id;

    // Create the args
    let args = vec![
        SuiJsonValue::new(json!("123"))?,
        SuiJsonValue::new(json!(address1))?,
    ];

    // Test case with no gas specified
    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
        gas_price: None,
    }
    .execute(context)
    .await?;
    resp.print(true);

    // Get the created object
    let created_obj: ObjectID = if let SuiClientCommandResult::TransactionBlock(resp) = resp {
        resp.effects
            .unwrap()
            .created()
            .first()
            .unwrap()
            .reference
            .object_id
    } else {
        panic!();
    };

    // Try a bad argument: decimal
    let args_json = json!([0.3f32, address1]);
    assert!(SuiJsonValue::new(args_json.as_array().unwrap().first().unwrap().clone()).is_err());

    // Try a bad argument: too few args
    let args_json = json!([300usize]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
        gas_price: None,
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    let err_string = format!("{} ", resp.err().unwrap());
    assert!(err_string.contains("Expected 2 args, found 1"));

    // Try a transfer
    // This should fail due to mismatch of object being sent
    let args = [
        SuiJsonValue::new(json!(obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
        gas_price: None,
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    // Try a transfer with explicitly set gas price.
    // It should fail due to that gas price is below RGP.
    let args = [
        SuiJsonValue::new(json!(created_obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
        gas_price: Some(1),
    }
    .execute(context)
    .await;

    assert!(resp.is_err());
    let err_string = format!("{} ", resp.err().unwrap());
    assert!(err_string.contains("Gas price 1 under reference gas price"));

    // FIXME: uncomment once we figure out what is going on with `resolve_and_type_check`
    // let err_string = format!("{} ", resp.err().unwrap());
    // let framework_addr = SUI_FRAMEWORK_ADDRESS.to_hex_literal();
    // let package_addr = package.to_hex_literal();
    // assert!(err_string.contains(&format!("Expected argument of type {package_addr}::object_basics::Object, but found type {framework_addr}::coin::Coin<{framework_addr}::sui::SUI>")));

    // Try a proper transfer
    let args = [
        SuiJsonValue::new(json!(created_obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        gas_price: None,
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
    }
    .execute(context)
    .await?;

    // Try a call with customized gas price.
    let args = vec![
        SuiJsonValue::new(json!("123"))?,
        SuiJsonValue::new(json!(address1))?,
    ];

    let result = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
        gas_price: Some(12345),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(txn_response) = result {
        assert_eq!(
            txn_response.transaction.unwrap().data.gas_data().price,
            12345
        );
    } else {
        panic!("Command failed with unexpected result.")
    };

    Ok(())
}

#[sim_test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .as_ref()
            .unwrap()
            .created()
            .iter()
            .map(|refe| refe.reference.object_id)
            .collect::<Vec<_>>()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_management_on_publish_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let build_config = BuildConfig::new_for_testing().config;
    // Publish the package
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config: build_config.clone(),
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    // Get Package ID and version
    let (expect_original_id, expect_version, _) =
        if let SuiClientCommandResult::TransactionBlock(response) = resp {
            assert_eq!(
                response.effects.as_ref().unwrap().gas_object().object_id(),
                gas_obj_id
            );
            get_new_package_obj_from_response(&response)
                .ok_or_else(|| anyhow::anyhow!("No package object response"))?
        } else {
            unreachable!("Invalid response");
        };

    // Get lock file that recorded Package ID and version
    let lock_file = build_config.lock_file.expect("Lock file for testing");
    let mut lock_file = std::fs::File::open(lock_file).unwrap();
    let envs = ManagedPackage::read(&mut lock_file).unwrap();
    let localnet = envs.get("localnet").unwrap();
    assert_eq!(
        expect_original_id.to_string(),
        localnet.original_published_id,
    );
    assert_eq!(expect_original_id.to_string(), localnet.latest_published_id);
    assert_eq!(
        expect_version.value(),
        localnet.version.parse::<u64>().unwrap(),
    );
    Ok(())
}

#[sim_test]
async fn test_delete_shared_object() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("sod");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "sod".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await?;

    let shared_id = if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
        response.effects.unwrap().created().to_vec()[0]
            .reference
            .object_id
    } else {
        unreachable!("Invalid response");
    };

    let delete_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "sod".to_string(),
        function: "delete".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![SuiJsonValue::from_str(&shared_id.to_string()).unwrap()],
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = delete_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "receiver".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument_by_immut_ref() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        gas_price: None,
        args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "invalid_call_immut_ref".to_string(),
        type_args: vec![],
        gas_price: None,
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument_by_mut_ref() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        skip_dependency_verification: false,
        with_unpublished_dependencies: false,
        verify_deps: true,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        gas_price: None,
        args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: (*package_id.object_id).into(),
        module: "tto".to_string(),
        function: "invalid_call_mut_ref".to_string(),
        type_args: vec![],
        gas_price: None,
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_package_publish_command_with_unpublished_dependency_succeeds(
) -> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = true; // Value under test, results in successful response.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object()?.object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .as_ref()
            .unwrap()
            .created()
            .iter()
            .map(|refe| refe.reference.object_id)
            .collect::<Vec<_>>()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_publish_command_with_unpublished_dependency_fails(
) -> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = false; // Value under test, results in error response.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies,
    }
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "Package dependency \"Unpublished\" does not specify a published address (the Move.toml manifest for \"Unpublished\" does not contain a 'published-at' field, nor is there a 'published-id' in the Move.lock).\nIf this is intentional, you may use the --with-unpublished-dependencies flag to continue publishing these dependencies as part of your package (they won't be linked against existing packages on-chain).",
            },
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_publish_command_non_zero_unpublished_dep_fails() -> Result<(), anyhow::Error>
{
    let with_unpublished_dependencies = true; // Value under test, incompatible with dependencies that specify non-zero address.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(address, None, None, None)
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency_with_non_zero_address");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies,
    }
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "The following modules in package dependencies set a non-zero self-address:\n - 0000000000000000000000000000000000000000000000000000000000000bad::non_zero in dependency UnpublishedNonZeroAddress\nIf these packages really are unpublished, their self-addresses should be set to \"0x0\" in the [addresses] section of the manifest when publishing. If they are already published, ensure they specify the address in the `published-at` of their Move.toml manifest.",
            },
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_publish_command_failure_invalid() -> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = true; // Invalid packages should fail to publish, even if we allow unpublished dependencies.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_failure_invalid");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies,
    }
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "Package dependency \"Invalid\" does not specify a valid published address: could not parse value \"mystery\" for 'published-at' field in Move.toml or 'published-id' in Move.lock file.",
            },
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_publish_nonexistent_dependency() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(address, None, None, None)
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_nonexistent_dependency");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await;

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Dependency object does not exist or was deleted"),
        "{}",
        err
    );
    Ok(())
}

#[sim_test]
async fn test_package_publish_test_flag() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(address, None, None, None)
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_nonexistent_dependency");
    let mut build_config: MoveBuildConfig = BuildConfig::new_for_testing().config;
    // this would have been the result of calling `sui client publish --test`
    build_config.test_mode = true;

    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "The `publish` subcommand should not be used with the `--test` flag\n\nCode in published packages must not depend on test code.\nIn order to fix this and publish the package without `--test`, remove any non-test dependencies on test-only code.\nYou can ensure all test-only dependencies have been removed by compiling the package normally with `sui move build`.",
            },
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_upgrade_command() -> Result<(), anyhow::Error> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_upgrade");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path: package_path.clone(),
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();

    let cap = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::AddressOwner(_)))
        .unwrap();

    // Hacky for now: we need to add the correct `published-at` field to the Move toml file.
    // In the future once we have automated address management replace this logic!
    let tmp_dir = tempfile::tempdir().unwrap();
    fs_extra::dir::copy(
        &package_path,
        tmp_dir.path(),
        &fs_extra::dir::CopyOptions::default(),
    )
    .unwrap();
    let mut upgrade_pkg_path = tmp_dir.path().to_path_buf();
    upgrade_pkg_path.extend(["dummy_modules_upgrade", "Move.toml"]);
    let mut move_toml = std::fs::File::options()
        .read(true)
        .write(true)
        .open(&upgrade_pkg_path)
        .unwrap();
    upgrade_pkg_path.pop();

    let mut buf = String::new();
    move_toml.read_to_string(&mut buf).unwrap();

    // Add a `published-at = "0x<package_object_id>"` to the Move manifest.
    let mut lines: Vec<String> = buf.split('\n').map(|x| x.to_string()).collect();
    let idx = lines.iter().position(|s| s == "[package]").unwrap();
    lines.insert(
        idx + 1,
        format!(
            "published-at = \"{}\"",
            package.reference.object_id.to_hex_uncompressed()
        ),
    );
    let new = lines.join("\n");
    move_toml.write_at(new.as_bytes(), 0).unwrap();

    // Now run the upgrade
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Upgrade {
        package_path: upgrade_pkg_path,
        upgrade_capability: cap.reference.object_id,
        build_config,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        verify_compatibility: true,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid upgrade response");
    };
    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);

    let obj_ids = effects
        .created()
        .iter()
        .map(|refe| refe.reference.object_id)
        .collect::<Vec<_>>();

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_management_on_upgrade_command() -> Result<(), anyhow::Error> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_upgrade");
    let mut build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path: package_path.clone(),
        build_config: build_config.clone(),
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let cap = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::AddressOwner(_)))
        .unwrap();

    // We will upgrade the package in a `tmp_dir` using the `Move.lock` resulting from publish,
    // so as not to clobber anything.
    // The `Move.lock` needs to point to the root directory of the package-to-be-upgraded.
    // The core implementation does not use support an arbitrary `lock_file` path specified in
    // `BuildConfig` when the `Move.lock` file is an input for upgrades, so we change the `BuildConfig`
    // `lock_file` to point to the root directory of package-to-be-upgraded.
    let tmp_dir = tempfile::tempdir().unwrap();
    fs_extra::dir::copy(
        &package_path,
        tmp_dir.path(),
        &fs_extra::dir::CopyOptions::default(),
    )
    .unwrap();
    let mut upgrade_pkg_path = tmp_dir.path().to_path_buf();
    upgrade_pkg_path.extend(["dummy_modules_upgrade", "Move.toml"]);
    upgrade_pkg_path.pop();
    // Place the `Move.lock` after publishing in the tmp dir for upgrading.
    let published_lock_file_path = build_config.lock_file.clone().unwrap();
    let mut upgrade_lock_file_path = upgrade_pkg_path.clone();
    upgrade_lock_file_path.push("Move.lock");
    std::fs::copy(
        published_lock_file_path.clone(),
        upgrade_lock_file_path.clone(),
    )?;
    // Point the `BuildConfig` lock_file to the package root.
    build_config.lock_file = Some(upgrade_pkg_path.join("Move.lock"));

    // Now run the upgrade
    let upgrade_response = SuiClientCommands::Upgrade {
        package_path: upgrade_pkg_path,
        upgrade_capability: cap.reference.object_id,
        build_config: build_config.clone(),
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        verify_compatibility: true,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    // Get Original Package ID and version
    let (expect_original_id, _, _) = get_new_package_obj_from_response(&publish_response)
        .ok_or_else(|| anyhow::anyhow!("No package object response"))?;

    // Get Upgraded Package ID and version
    let (expect_upgrade_latest_id, expect_upgrade_version, _) =
        if let SuiClientCommandResult::TransactionBlock(response) = upgrade_response {
            assert_eq!(
                response.effects.as_ref().unwrap().gas_object().object_id(),
                gas_obj_id
            );
            get_new_package_obj_from_response(&response)
                .ok_or_else(|| anyhow::anyhow!("No package object response"))?
        } else {
            unreachable!("Invalid response");
        };

    // Get lock file that recorded Package ID and version
    let lock_file = build_config.lock_file.expect("Lock file for testing");
    let mut lock_file = std::fs::File::open(lock_file).unwrap();
    let envs = ManagedPackage::read(&mut lock_file).unwrap();
    let localnet = envs.get("localnet").unwrap();
    // Original ID should correspond to first published package.
    assert_eq!(
        expect_original_id.to_string(),
        localnet.original_published_id,
    );
    // Upgrade ID should correspond to upgraded package.
    assert_eq!(
        expect_upgrade_latest_id.to_string(),
        localnet.latest_published_id,
    );
    // Version should correspond to upgraded package.
    assert_eq!(
        expect_upgrade_version.value(),
        localnet.version.parse::<u64>().unwrap(),
    );
    Ok(())
}

#[sim_test]
async fn test_package_management_on_upgrade_command_conflict() -> Result<(), anyhow::Error> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_upgrade");
    let build_config_publish = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path: package_path.clone(),
        build_config: build_config_publish.clone(),
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();

    let cap = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::AddressOwner(_)))
        .unwrap();

    // Set up a temporary working directory  for upgrading.
    let tmp_dir = tempfile::tempdir().unwrap();
    fs_extra::dir::copy(
        &package_path,
        tmp_dir.path(),
        &fs_extra::dir::CopyOptions::default(),
    )
    .unwrap();
    let mut upgrade_pkg_path = tmp_dir.path().to_path_buf();
    upgrade_pkg_path.extend(["dummy_modules_upgrade", "Move.toml"]);
    let mut move_toml = std::fs::File::options()
        .read(true)
        .write(true)
        .open(&upgrade_pkg_path)
        .unwrap();
    upgrade_pkg_path.pop();
    let mut buf = String::new();
    move_toml.read_to_string(&mut buf).unwrap();
    let mut lines: Vec<String> = buf.split('\n').map(|x| x.to_string()).collect();
    let idx = lines.iter().position(|s| s == "[package]").unwrap();
    // Purposely add a conflicting `published-at` address to the Move manifest.
    lines.insert(idx + 1, "published-at = \"0xbad\"".to_string());
    let new = lines.join("\n");
    move_toml.write_at(new.as_bytes(), 0).unwrap();

    // Create a new build config for the upgrade. Initialize its lock file to the package we published.
    let build_config_upgrade = BuildConfig::new_for_testing().config;
    let mut upgrade_lock_file_path = upgrade_pkg_path.clone();
    upgrade_lock_file_path.push("Move.lock");
    let publish_lock_file_path = build_config_publish.lock_file.unwrap();
    std::fs::copy(
        publish_lock_file_path.clone(),
        upgrade_lock_file_path.clone(),
    )?;

    // Now run the upgrade
    let upgrade_response = SuiClientCommands::Upgrade {
        package_path: upgrade_pkg_path,
        upgrade_capability: cap.reference.object_id,
        build_config: build_config_upgrade.clone(),
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        verify_compatibility: true,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
    }
    .execute(context)
    .await;

    let err_string = upgrade_response.unwrap_err().to_string();
    let err_string = err_string.replace(&package.object_id().to_string(), "<elided-for-test>");

    let expect = expect![[r#"
        Conflicting published package address: `Move.toml` contains published-at address 0x0000000000000000000000000000000000000000000000000000000000000bad but `Move.lock` file contains published-at address <elided-for-test>. You may want to:

                         - delete the published-at address in the `Move.toml` if the `Move.lock` address is correct; OR
                         - update the `Move.lock` address using the `sui manage-package` command to be the same as the `Move.toml`; OR
                         - check that your `sui active-env` (currently localnet) corresponds to the chain on which the package is published (i.e., devnet, testnet, mainnet); OR
                         - contact the maintainer if this package is a dependency and request resolving the conflict."#]];
    expect.assert_eq(&err_string);
    Ok(())
}

#[sim_test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let recipient = SuiAddress::random_for_testing_only();
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;
    let obj_id = object_refs.get(1).unwrap().object().unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
        to: KeyIdentity::Address(recipient),
        object_id: obj_id,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (mut_obj1, mut_obj2) = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert!(
            response.status_ok().unwrap(),
            "Command failed: {:?}",
            response
        );
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        (
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .first()
                .unwrap()
                .reference
                .object_id,
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .get(1)
                .unwrap()
                .reference
                .object_id,
        )
    } else {
        panic!()
    };

    // Check the objects
    let resp = SuiClientCommands::Object {
        id: mut_obj1,
        bcs: false,
    }
    .execute(context)
    .await?;
    let mut_obj1 = if let SuiClientCommandResult::Object(resp) = resp {
        if let Some(obj) = resp.data {
            obj
        } else {
            panic!()
        }
    } else {
        panic!();
    };

    let resp2 = SuiClientCommands::Object {
        id: mut_obj2,
        bcs: false,
    }
    .execute(context)
    .await?;
    let mut_obj2 = if let SuiClientCommandResult::Object(resp2) = resp2 {
        if let Some(obj) = resp2.data {
            obj
        } else {
            panic!()
        }
    } else {
        panic!();
    };

    let (gas, obj) = if mut_obj1.owner.clone().unwrap().get_owner_address().unwrap() == address {
        (mut_obj1, mut_obj2)
    } else {
        (mut_obj2, mut_obj1)
    };

    assert_eq!(gas.owner.unwrap().get_owner_address().unwrap(), address);
    assert_eq!(obj.owner.unwrap().get_owner_address().unwrap(), recipient);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let obj_id = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
        to: KeyIdentity::Address(recipient),
        object_id: obj_id,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (_mut_obj1, _mut_obj2) = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        (
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .first()
                .unwrap()
                .reference
                .object_id,
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .get(1)
                .unwrap()
                .reference
                .object_id,
        )
    } else {
        panic!()
    };

    Ok(())
}

#[test]
// Test for issue https://github.com/MystenLabs/sui/issues/1078
fn test_bug_1078() {
    let read = SuiClientCommandResult::Object(SuiObjectResponse::new_with_error(
        SuiObjectResponseError::NotExists {
            object_id: ObjectID::random(),
        },
    ));
    let mut writer = String::new();
    // fmt ObjectRead should not fail.
    write!(writer, "{}", read).unwrap();
    write!(writer, "{:?}", read).unwrap();
}

#[sim_test]
async fn test_switch_command() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let addr2 = cluster.get_address_1();
    let context = cluster.wallet_mut();

    // Get the active address
    let addr1 = context.active_address()?;

    // Run a command with address omitted
    let os = SuiClientCommands::Objects { address: None }
        .execute(context)
        .await?;

    let mut cmd_objs = if let SuiClientCommandResult::Objects(v) = os {
        v
    } else {
        panic!("Command failed")
    };

    // Check that we indeed fetched for addr1
    let client = context.get_client().await?;
    let mut actual_objs = client
        .read_api()
        .get_owned_objects(
            addr1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await
        .unwrap()
        .data;
    cmd_objs.sort();
    actual_objs.sort();
    assert_eq!(cmd_objs, actual_objs);

    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(addr2)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(addr2, context.active_address()?);
    assert_ne!(addr1, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2.to_string()),
                env: None
            })
        )
    );

    // Wipe all the address info
    context.config.active_address = None;

    // Create a new address
    let os = SuiClientCommands::NewAddress {
        key_scheme: SignatureScheme::ED25519,
        alias: None,
        derivation_path: None,
        word_length: None,
    }
    .execute(context)
    .await?;
    let new_addr = if let SuiClientCommandResult::NewAddress(x) = os {
        x.address
    } else {
        panic!("Command failed")
    };

    // Check that we can switch to this address
    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(new_addr)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(new_addr, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(new_addr.to_string()),
                env: None
            })
        )
    );
    Ok(())
}

#[sim_test]
async fn test_new_address_command_by_flag() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = cluster.wallet_mut();

    // keypairs loaded from config are Ed25519
    assert_eq!(
        context
            .config
            .keystore
            .keys()
            .iter()
            .filter(|k| k.flag() == Ed25519SuiSignature::SCHEME.flag())
            .count(),
        5
    );

    SuiClientCommands::NewAddress {
        key_scheme: SignatureScheme::Secp256k1,
        alias: None,
        derivation_path: None,
        word_length: None,
    }
    .execute(context)
    .await?;

    // new keypair generated is Secp256k1
    assert_eq!(
        context
            .config
            .keystore
            .keys()
            .iter()
            .filter(|k| k.flag() == Secp256k1SuiSignature::SCHEME.flag())
            .count(),
        1
    );

    Ok(())
}

#[sim_test]
async fn test_active_address_command() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = cluster.wallet_mut();

    // Get the active address
    let addr1 = context.active_address()?;

    // Run a command with address omitted
    let os = SuiClientCommands::ActiveAddress {}.execute(context).await?;

    let a = if let SuiClientCommandResult::ActiveAddress(Some(v)) = os {
        v
    } else {
        panic!("Command failed")
    };
    assert_eq!(a, addr1);

    let addr2 = context.config.keystore.addresses().get(1).cloned().unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(addr2)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2.to_string()),
                env: None
            })
        )
    );

    // switch back to addr1 by using its alias
    let alias1 = context
        .config
        .keystore
        .get_alias_by_address(&addr1)
        .unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Alias(alias1)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr1.to_string()),
                env: None
            })
        )
    );

    Ok(())
}

fn get_gas_value(o: &SuiObjectData) -> u64 {
    GasCoin::try_from(o).unwrap().value()
}

async fn get_object(id: ObjectID, context: &WalletContext) -> Option<SuiObjectData> {
    let client = context.get_client().await.unwrap();
    let response = client
        .read_api()
        .get_object_with_options(id, SuiObjectDataOptions::full_content())
        .await
        .unwrap();
    response.data
}

async fn get_parsed_object_assert_existence(
    object_id: ObjectID,
    context: &WalletContext,
) -> SuiObjectData {
    get_object(object_id, context)
        .await
        .expect("Object {object_id} does not exist.")
}

#[sim_test]
async fn test_merge_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object().unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object().unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC),
    }
    .execute(context)
    .await?;
    let g = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        assert_eq!(r.effects.as_ref().unwrap().gas_object().object_id(), gas);
        let object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        get_parsed_object_assert_existence(object_id, context).await
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, context).await, None);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    let primary_coin = object_refs.data.get(1).unwrap().object()?.object_id;
    let coin_to_merge = object_refs.data.get(2).unwrap().object()?.object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC),
    }
    .execute(context)
    .await?;

    let g = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        let object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        get_parsed_object_assert_existence(object_id, context).await
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, context).await, None);

    Ok(())
}

#[sim_test]
async fn test_split_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas = object_refs.data.first().unwrap().object()?.object_id;
    let mut coin = object_refs.data.get(1).unwrap().object()?.object_id;

    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::SplitCoin {
        opts: OptsWithGas::for_testing(Some(gas), rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        assert_eq!(r.effects.as_ref().unwrap().gas_object().object_id(), gas);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&new_coins[0]) == 1000) || (get_gas_value(&new_coins[0]) == 10));
    assert!((get_gas_value(&new_coins[1]) == 1000) || (get_gas_value(&new_coins[1]) == 10));
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Get another coin
    for c in object_refs {
        let coin_data = c.into_object().unwrap();
        if get_gas_value(&get_object(coin_data.object_id, context).await.unwrap()) > 2000 {
            coin = coin_data.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test split coin into equal parts
    let resp = SuiClientCommands::SplitCoin {
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
        coin_id: coin,
        amounts: None,
        count: Some(3),
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(
        get_gas_value(&updated_coin),
        orig_value / 3 + orig_value % 3
    );
    assert_eq!(get_gas_value(&new_coins[0]), orig_value / 3);
    assert_eq!(get_gas_value(&new_coins[1]), orig_value / 3);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Get another coin
    for c in object_refs {
        let coin_data = c.into_object().unwrap();
        if get_gas_value(&get_object(coin_data.object_id, context).await.unwrap()) > 2000 {
            coin = coin_data.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::SplitCoin {
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&new_coins[0]) == 1000) || (get_gas_value(&new_coins[0]) == 10));
    assert!((get_gas_value(&new_coins[1]) == 1000) || (get_gas_value(&new_coins[1]) == 10));
    Ok(())
}

#[sim_test]
async fn test_signature_flag() -> Result<(), anyhow::Error> {
    let res = SignatureScheme::from_flag("0");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::ED25519.flag());

    let res = SignatureScheme::from_flag("1");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::Secp256k1.flag());

    let res = SignatureScheme::from_flag("2");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::Secp256r1.flag());

    let res = SignatureScheme::from_flag("something");
    assert!(res.is_err());
    Ok(())
}

#[sim_test]
async fn test_execute_signed_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let mut txns = batch_make_transfer_transactions(context, 1).await;
    let txn = txns.swap_remove(0);

    let (tx_data, signatures) = txn.to_tx_bytes_and_signatures();
    SuiClientCommands::ExecuteSignedTx {
        tx_bytes: tx_data.encoded(),
        signatures: signatures.into_iter().map(|s| s.encoded()).collect(),
    }
    .execute(context)
    .await?;
    Ok(())
}

#[sim_test]
async fn test_serialize_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let address1 = test_cluster.get_address_1();
    let context = &mut test_cluster.wallet;
    let alias1 = context
        .config
        .keystore
        .get_alias_by_address(&address1)
        .unwrap();
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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
    let coin = object_refs.get(1).unwrap().object().unwrap().object_id;

    SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address1),
        sui_coin_object_id: coin,
        amount: Some(1),
        opts: Opts {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            dry_run: false,
            dev_inspect: false,
            serialize_unsigned_transaction: true,
            serialize_signed_transaction: false,
        },
    }
    .execute(context)
    .await?;

    SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address1),
        sui_coin_object_id: coin,
        amount: Some(1),
        opts: Opts {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            dry_run: false,
            dev_inspect: false,
            serialize_unsigned_transaction: false,
            serialize_signed_transaction: true,
        },
    }
    .execute(context)
    .await?;

    // use alias for transfer
    SuiClientCommands::TransferSui {
        to: KeyIdentity::Alias(alias1),
        sui_coin_object_id: coin,
        amount: Some(1),
        opts: Opts {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            dry_run: false,
            dev_inspect: false,
            serialize_unsigned_transaction: false,
            serialize_signed_transaction: true,
        },
    }
    .execute(context)
    .await?;

    let ptb_args = vec![
        "--split-coins".to_string(),
        "gas".to_string(),
        "[1000]".to_string(),
        "--assign".to_string(),
        "new_coin".to_string(),
        "--transfer-objects".to_string(),
        "[new_coin]".to_string(),
        format!("@{}", address1),
        "--gas-budget".to_string(),
        "50000000".to_string(),
    ];
    let mut args = ptb_args.clone();
    args.push("--serialize-signed-transaction".to_string());
    let ptb = PTB { args };
    SuiClientCommands::PTB(ptb).execute(context).await.unwrap();
    let mut args = ptb_args.clone();
    args.push("--serialize-unsigned-transaction".to_string());
    let ptb = PTB { args };
    SuiClientCommands::PTB(ptb).execute(context).await.unwrap();

    Ok(())
}

#[tokio::test]
async fn test_stake_with_none_amount() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await?
        .data;

    let config_path = test_cluster.swarm.dir().join(SUI_CLIENT_CONFIG);
    let validator_addr = client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    test_with_sui_binary(&[
        "client",
        "--client.config",
        config_path.to_str().unwrap(),
        "call",
        "--package",
        "0x3",
        "--module",
        "sui_system",
        "--function",
        "request_add_stake_mul_coin",
        "--args",
        "0x5",
        &format!("[{}]", coins.first().unwrap().coin_object_id),
        "[]",
        &validator_addr.to_string(),
        "--gas-budget",
        "1000000000",
    ])
    .await?;

    let stake = client.governance_api().get_stakes(address).await?;

    assert_eq!(1, stake.len());
    assert_eq!(
        coins.first().unwrap().balance,
        stake.first().unwrap().stakes.first().unwrap().principal
    );
    Ok(())
}

#[tokio::test]
async fn test_stake_with_u64_amount() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await?
        .data;

    let config_path = test_cluster.swarm.dir().join(SUI_CLIENT_CONFIG);
    let validator_addr = client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    test_with_sui_binary(&[
        "client",
        "--client.config",
        config_path.to_str().unwrap(),
        "call",
        "--package",
        "0x3",
        "--module",
        "sui_system",
        "--function",
        "request_add_stake_mul_coin",
        "--args",
        "0x5",
        &format!("[{}]", coins.first().unwrap().coin_object_id),
        "[1000000000]",
        &validator_addr.to_string(),
        "--gas-budget",
        "1000000000",
    ])
    .await?;

    let stake = client.governance_api().get_stakes(address).await?;

    assert_eq!(1, stake.len());
    assert_eq!(
        1000000000,
        stake.first().unwrap().stakes.first().unwrap().principal
    );
    Ok(())
}

async fn test_with_sui_binary(args: &[&str]) -> Result<(), anyhow::Error> {
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // test cluster will not response if this call is in the same thread
    let out = thread::spawn(move || cmd.args(args).assert());
    while !out.is_finished() {
        sleep(Duration::from_millis(100)).await;
    }
    out.join().unwrap().success();
    Ok(())
}

#[sim_test]
async fn test_get_owned_objects_owned_by_address_and_check_pagination() -> Result<(), anyhow::Error>
{
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_responses = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new(
                Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                Some(
                    SuiObjectDataOptions::new()
                        .with_type()
                        .with_owner()
                        .with_previous_transaction(),
                ),
            )),
            None,
            None,
        )
        .await?;

    // assert that all the objects_returned are owned by the address
    for resp in &object_responses.data {
        let obj_owner = resp.object().unwrap().owner.clone().unwrap();
        assert_eq!(
            obj_owner.get_owner_address().unwrap().to_string(),
            address.to_string()
        )
    }
    // assert that has next page is false
    assert!(!object_responses.has_next_page);

    // Pagination check
    let mut has_next = true;
    let mut cursor = None;
    let mut response_data: Vec<SuiObjectResponse> = Vec::new();
    while has_next {
        let object_responses = client
            .read_api()
            .get_owned_objects(
                address,
                Some(SuiObjectResponseQuery::new(
                    Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                    Some(
                        SuiObjectDataOptions::new()
                            .with_type()
                            .with_owner()
                            .with_previous_transaction(),
                    ),
                )),
                cursor,
                Some(1),
            )
            .await?;

        response_data.push(object_responses.data.first().unwrap().clone());

        if object_responses.has_next_page {
            cursor = object_responses.next_cursor;
        } else {
            has_next = false;
        }
    }

    assert_eq!(&response_data, &object_responses.data);

    Ok(())
}

#[tokio::test]
async fn test_linter_suppression_stats() -> Result<(), anyhow::Error> {
    const LINTER_MSG: &str = "Total number of linter warnings suppressed: 5 (unique lints: 3)";
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    let args = vec!["move", "test", "--path", "tests/data/linter"];
    let output = cmd
        .args(&args)
        .output()
        .expect("failed to run 'sui move test'");
    let out_str = str::from_utf8(&output.stderr).unwrap();
    assert!(
        out_str.contains(LINTER_MSG),
        "Expected to match {LINTER_MSG}, got: {out_str}"
    );
    // test no-lint suppresses
    let args = vec!["move", "test", "--no-lint", "--path", "tests/data/linter"];
    let output = cmd
        .args(&args)
        .output()
        .expect("failed to run 'sui move test'");
    let out_str = str::from_utf8(&output.stderr).unwrap();
    assert!(
        !out_str.contains(LINTER_MSG),
        "Expected _not to_ match {LINTER_MSG}, got: {out_str}"
    );
    Ok(())
}

#[tokio::test]
async fn key_identity_test() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context
        .config
        .keystore
        .get_alias_by_address(&address)
        .unwrap();

    // by alias
    assert_eq!(
        address,
        get_identity_address(Some(KeyIdentity::Alias(alias)), context).unwrap()
    );
    // by address
    assert_eq!(
        address,
        get_identity_address(Some(KeyIdentity::Address(address)), context).unwrap()
    );
    // alias does not exist
    assert!(get_identity_address(Some(KeyIdentity::Alias("alias".to_string())), context).is_err());

    // get active address instead when no alias/address is given
    assert_eq!(
        context.active_address().unwrap(),
        get_identity_address(None, context).unwrap()
    );
}

fn assert_dry_run(dry_run: SuiClientCommandResult, object_id: ObjectID, command: &str) {
    if let SuiClientCommandResult::DryRun(response) = dry_run {
        assert_eq!(
            *response.effects.status(),
            SuiExecutionStatus::Success,
            "{command} dry run test effects is not success"
        );
        assert_eq!(
            response.effects.gas_object().object_id(),
            object_id,
            "{command} dry run test failed, gas object used is not the expected one"
        );
    } else {
        panic!("{} dry run failed", command);
    }
}

#[sim_test]
async fn test_dry_run() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?;

    let object_id = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_to_send = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    // === TRANSFER === //
    let transfer_dry_run = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        object_id: object_to_send,
        opts: OptsWithGas::for_testing_dry_run(
            Some(object_id),
            rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        ),
    }
    .execute(context)
    .await?;

    assert_dry_run(transfer_dry_run, object_id, "Transfer");

    // === TRANSFER SUI === //
    let transfer_sui_dry_run = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        sui_coin_object_id: object_to_send,
        amount: Some(1),
        opts: Opts::for_testing_dry_run(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    assert_dry_run(transfer_sui_dry_run, object_to_send, "TransferSui");

    // === PAY === //
    let pay_dry_run = SuiClientCommands::Pay {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        opts: OptsWithGas::for_testing_dry_run(None, rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::DryRun(response) = pay_dry_run {
        assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
        assert_ne!(response.effects.gas_object().object_id(), object_id);
    } else {
        panic!("Pay dry run failed");
    }

    // specify which gas object to use
    let gas_coin_id = object_refs.data.last().unwrap().object().unwrap().object_id;
    let pay_dry_run = SuiClientCommands::Pay {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        opts: OptsWithGas::for_testing_dry_run(
            Some(gas_coin_id),
            rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        ),
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_dry_run, gas_coin_id, "Pay");

    // === PAY SUI === //
    let pay_sui_dry_run = SuiClientCommands::PaySui {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        opts: Opts::for_testing_dry_run(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_sui_dry_run, object_id, "PaySui");

    // === PAY ALL SUI === //
    let pay_all_sui_dry_run = SuiClientCommands::PayAllSui {
        input_coins: vec![object_id],
        recipient: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        opts: Opts::for_testing_dry_run(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_all_sui_dry_run, object_id, "PayAllSui");

    Ok(())
}

async fn test_cluster_helper() -> (
    TestCluster,
    SuiClient,
    u64,
    [ObjectID; 3],
    [KeyIdentity; 2],
    [SuiAddress; 2],
) {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address1 = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await.unwrap();
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await
        .unwrap();

    let object_id1 = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_id2 = object_refs.data.get(1).unwrap().object().unwrap().object_id;
    let object_id3 = object_refs.data.get(2).unwrap().object().unwrap().object_id;
    let address2 = SuiAddress::random_for_testing_only();
    let address3 = SuiAddress::random_for_testing_only();
    let recipient1 = KeyIdentity::Address(address2);
    let recipient2 = KeyIdentity::Address(address3);

    (
        test_cluster,
        client,
        rgp,
        [object_id1, object_id2, object_id3],
        [recipient1, recipient2],
        [address2, address3],
    )
}

#[sim_test]
async fn test_pay() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2, object_id3) = (objects[0], objects[1], objects[2]);
    let (recipient1, recipient2) = (&recipients[0], &recipients[1]);
    let (address2, address3) = (addresses[0], addresses[1]);
    let context = &mut test_cluster.wallet;
    let pay = SuiClientCommands::Pay {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: vec![5000, 10000],
        opts: OptsWithGas::for_testing(Some(object_id1), rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await;

    // we passed the gas object to be one of the input coins, which should fail
    assert!(pay.is_err());

    let amounts = [5000, 10000];
    // we expect this to be the gas coin used
    let pay = SuiClientCommands::Pay {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: amounts.into(),
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    // Pay command takes the input coins and transfers the given amounts from each input coin (in order)
    // to the recipients
    // this test checks if the recipients have received the objects, and if the gas object used is
    // the right one (not one of the input coins, and in this setup it's the 3rd coin of sender)
    // we also check if the balances are right!
    if let SuiClientCommandResult::TransactionBlock(response) = pay {
        // check tx status
        assert!(response.status_ok().unwrap());
        // check gas coin used
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id3
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address2, None)
                .await?
                .total_balance,
            amounts[0] as u128
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address3,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address3, None)
                .await?
                .total_balance,
            amounts[1] as u128
        );
    } else {
        panic!("Pay test failed");
    }

    Ok(())
}

#[sim_test]
async fn test_pay_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let (recipient1, recipient2) = (&recipients[0], &recipients[1]);
    let (address2, address3) = (addresses[0], addresses[1]);
    let context = &mut test_cluster.wallet;
    let amounts = [1000, 5000];
    let pay_sui = SuiClientCommands::PaySui {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: amounts.into(),
        opts: Opts::for_testing(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    // pay sui takes the input coins and transfers from each of them (in order) the amounts to the
    // respective receipients.
    // check if each recipient has one object, if the tx status is success,
    // and if the gas object used was the first object in the input coins
    // we also check if the balances of each recipient are right!
    if let SuiClientCommandResult::TransactionBlock(response) = pay_sui {
        assert!(response.status_ok().unwrap());
        // check gas coin used
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address2, None)
                .await?
                .total_balance,
            amounts[0] as u128
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address3,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address3, None)
                .await?
                .total_balance,
            amounts[1] as u128
        );
    } else {
        panic!("PaySui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_pay_all_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let pay_all_sui = SuiClientCommands::PayAllSui {
        input_coins: vec![object_id1, object_id2],
        recipient: recipient1.clone(),
        opts: Opts::for_testing(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    // pay all sui will take the input coins and smash them into one coin and transfer that coin to
    // the recipient, so we check that the recipient has one object, if the tx status is success,
    // and if the gas object used was the first object in the input coins
    if let SuiClientCommandResult::TransactionBlock(response) = pay_all_sui {
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            response.effects.unwrap().gas_object().object_id(),
            object_id1
        );
    } else {
        panic!("PayAllSui test failed");
    }

    Ok(())
}

#[sim_test]
async fn test_transfer() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(address2),
        object_id: object_id1,
        opts: OptsWithGas::for_testing(Some(object_id1), rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await;

    // passed the gas object to be the object to transfer, which should fail
    assert!(transfer.is_err());

    let transfer = SuiClientCommands::Transfer {
        to: recipient1.clone(),
        object_id: object_id1,
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;
    // transfer command will transfer the object_id1 to address2, and use object_id2 as gas
    // we check if object1 is owned by address 2 and if the gas object used is object_id2
    if let SuiClientCommandResult::TransactionBlock(response) = transfer {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id2
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            objs_refs.data.first().unwrap().object().unwrap().object_id,
            object_id1
        );
    } else {
        panic!("Transfer test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_transfer_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let object_id1 = objects[0];
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let amount = 1000;
    let transfer_sui = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address2),
        sui_coin_object_id: object_id1,
        amount: Some(amount),
        opts: Opts::for_testing(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;

    // transfer sui will transfer the amount from object_id1 to address2, and use the same object
    // as gas, and we check if the recipient address received the object, and the expected balance
    // is correct
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        let balance = client
            .coin_read_api()
            .get_balance(address2, None)
            .await?
            .total_balance;
        assert_eq!(balance, amount as u128);
    } else {
        panic!("TransferSui test failed");
    }
    // transfer the whole object by not passing an amount
    let transfer_sui = SuiClientCommands::TransferSui {
        to: recipient1.clone(),
        sui_coin_object_id: object_id1,
        amount: None,
        opts: Opts::for_testing(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(
            objs_refs.data.len(),
            2,
            "Expected to have two coins when calling transfer sui the 2nd time"
        );
        assert!(objs_refs
            .data
            .iter()
            .any(|x| x.object().unwrap().object_id == object_id1));
    } else {
        panic!("TransferSui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_gas_estimation() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, _, addresses) = test_cluster_helper().await;
    let object_id1 = objects[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let amount = 1000;
    let sender = context.active_address().unwrap();
    let tx_builder = client.transaction_builder();
    let tx_kind = tx_builder.transfer_sui_tx_kind(address2, Some(amount));
    let gas_estimate = estimate_gas_budget(context, sender, tx_kind, rgp, None, None).await;
    assert!(gas_estimate.is_ok());

    let transfer_sui_cmd = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address2),
        sui_coin_object_id: object_id1,
        amount: Some(amount),
        opts: Opts {
            gas_budget: None,
            dry_run: false,
            dev_inspect: false,
            serialize_unsigned_transaction: false,
            serialize_signed_transaction: false,
        },
    }
    .execute(context)
    .await
    .unwrap();
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui_cmd {
        assert!(response.status_ok().unwrap());
        let gas_used = response.effects.as_ref().unwrap().gas_object().object_id();
        assert_eq!(gas_used, object_id1);
        assert!(
            response
                .effects
                .as_ref()
                .unwrap()
                .gas_cost_summary()
                .gas_used()
                <= gas_estimate.unwrap()
        );
    } else {
        panic!("TransferSui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_clever_errors() -> Result<(), anyhow::Error> {
    // Publish the package
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
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

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("clever_errors");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish {
        package_path: package_path.clone(),
        build_config,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        opts: OptsWithGas::for_testing(Some(gas_obj_id), rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();

    let elide_transaction_digest = |s: String| -> String {
        let mut x = s.splitn(5, '\'').collect::<Vec<_>>();
        x[1] = "ELIDED_TRANSACTION_DIGEST";
        let tmp = format!("ELIDED_ADDRESS{}", &x[3][66..]);
        x[3] = &tmp;
        x.join("'")
    };

    // Normal abort
    let non_clever_abort = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "aborter".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await
    .unwrap_err();

    // Line-only abort
    let line_only_abort = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "aborter_line_no".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await
    .unwrap_err();

    // Full clever error with utf-8 string
    let clever_error_utf8 = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "clever_aborter".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await
    .unwrap_err();

    // Full clever error with non-utf-8 string
    let clever_error_non_utf8 = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "clever_aborter_not_a_string".to_string(),
        type_args: vec![],
        opts: OptsWithGas::for_testing(None, rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
        gas_price: None,
        args: vec![],
    }
    .execute(context)
    .await
    .unwrap_err();

    let error_string = format!(
        "Non-clever-abort\n---\n{}\n---\nLine-only-abort\n---\n{}\n---\nClever-error-utf8\n---\n{}\n---\nClever-error-non-utf8\n---\n{}\n---\n",
        elide_transaction_digest(non_clever_abort.to_string()),
        elide_transaction_digest(line_only_abort.to_string()),
        elide_transaction_digest(clever_error_utf8.to_string()),
        elide_transaction_digest(clever_error_non_utf8.to_string())
    );

    insta::assert_snapshot!(error_string);
    Ok(())
}

#[tokio::test]
async fn test_parse_host_port() {
    let input = "127.0.0.0";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "127.0.0.0:9123".parse::<SocketAddr>().unwrap());

    let input = "127.0.0.5:9124";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "127.0.0.5:9124".parse::<SocketAddr>().unwrap());

    let input = "9090";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "0.0.0.0:9090".parse::<SocketAddr>().unwrap());

    let input = "";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "0.0.0.0:9123".parse::<SocketAddr>().unwrap());

    let result = parse_host_port("localhost".to_string(), 9899).unwrap();
    assert_eq!(result, "127.0.0.1:9899".parse::<SocketAddr>().unwrap());

    let input = "asg";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.0.0:900";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.0.0";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.9.0.1:asb";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
}
