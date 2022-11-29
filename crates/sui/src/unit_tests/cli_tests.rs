// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Write, fs::read_dir, path::PathBuf, str, time::Duration};

use anyhow::anyhow;
use move_package::BuildConfig;
use serde_json::json;

use sui::client_commands::SwitchResponse;
use sui::{
    client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext},
    config::SuiClientConfig,
    sui_commands::SuiCommand,
};
use sui_config::genesis_config::{AccountConfig, GenesisConfig, ObjectConfig};
use sui_config::{
    NetworkConfig, PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG, SUI_GENESIS_FILENAME,
    SUI_KEYSTORE_FILENAME, SUI_NETWORK_CONFIG,
};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GetObjectDataResponse, SuiData, SuiObject, SuiParsedData, SuiParsedObject,
    SuiTransactionEffects,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    Ed25519SuiSignature, Secp256k1SuiSignature, SignatureScheme, SuiKeyPair, SuiSignatureInner,
};
use sui_types::{base_types::ObjectID, crypto::get_key_pair, gas_coin::GasCoin};
use sui_types::{sui_framework_address_concat_string, SUI_FRAMEWORK_ADDRESS};
use test_utils::messages::make_transactions_with_wallet_context;
use test_utils::network::init_cluster_builder_env_aware;

const TEST_DATA_DIR: &str = "src/unit_tests/data/";

#[sim_test]
async fn test_genesis() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    let config = working_dir.join(SUI_NETWORK_CONFIG);

    // Start network without authorities
    let start = SuiCommand::Start {
        config: Some(config),
        no_full_node: false,
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
    }
    .execute()
    .await?;

    // Get all the new file names
    let files = read_dir(working_dir)?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(9, files.len());
    assert!(files.contains(&SUI_CLIENT_CONFIG.to_string()));
    assert!(files.contains(&SUI_NETWORK_CONFIG.to_string()));
    assert!(files.contains(&SUI_FULLNODE_CONFIG.to_string()));
    assert!(files.contains(&SUI_GENESIS_FILENAME.to_string()));

    assert!(files.contains(&SUI_KEYSTORE_FILENAME.to_string()));

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
    }
    .execute()
    .await;
    assert!(matches!(result, Err(..)));

    temp_dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    let test_cluster = init_cluster_builder_env_aware().build().await?;
    let mut context = test_cluster.wallet;

    // Add 3 accounts
    for _ in 0..3 {
        context
            .config
            .keystore
            .add_key(SuiKeyPair::Ed25519SuiKeyPair(get_key_pair().1))?;
    }

    // Print all addresses
    SuiClientCommands::Addresses
        .execute(&mut context)
        .await
        .unwrap()
        .print(true);

    Ok(())
}

#[sim_test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(address),
    }
    .execute(context)
    .await?
    .print(true);

    let _object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    Ok(())
}

#[sim_test]
async fn test_create_example_nft_command() {
    let mut test_cluster = init_cluster_builder_env_aware().build().await.unwrap();
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let result = SuiClientCommands::CreateExampleNFT {
        name: None,
        description: None,
        url: None,
        gas: None,
        gas_budget: None,
    }
    .execute(context)
    .await
    .unwrap();

    match result {
        SuiClientCommandResult::CreateExampleNFT(GetObjectDataResponse::Exists(obj)) => {
            assert_eq!(obj.owner, address);
            assert_eq!(
                obj.data.type_().unwrap(),
                sui_framework_address_concat_string("::devnet_nft::DevNetNFT")
            );
            Ok(obj)
        }
        _ => Err(anyhow!(
            "WalletCommands::CreateExampleNFT returns wrong type"
        )),
    }
    .unwrap();
}

#[sim_test]
async fn test_custom_genesis() -> Result<(), anyhow::Error> {
    // Create and save genesis config file
    // Create 4 authorities, 1 account with 1 gas object with custom id

    let mut config = GenesisConfig::for_local_testing();
    config.accounts.clear();
    let object_id = ObjectID::random();
    config.accounts.push(AccountConfig {
        address: None,
        gas_objects: vec![ObjectConfig {
            object_id,
            gas_value: 500,
        }],
        gas_object_ranges: None,
    });
    let mut cluster = init_cluster_builder_env_aware()
        .set_genesis_config(config)
        .build()
        .await?;
    let address = cluster.get_address_0();
    let context = cluster.wallet_mut();

    assert_eq!(1, context.config.keystore.addresses().len());

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(address),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;

    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().object_id;

    SuiClientCommands::Object { id: object_id }
        .execute(context)
        .await?
        .print(true);

    Ok(())
}

#[sim_test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    let object_id = object_refs.first().unwrap().object_id;
    let object_to_send = object_refs.get(1).unwrap().object_id;

    SuiClientCommands::Gas {
        address: Some(address),
    }
    .execute(context)
    .await?
    .print(true);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send an object
    SuiClientCommands::Transfer {
        to: SuiAddress::random_for_testing_only(),
        object_id: object_to_send,
        gas: Some(object_id),
        gas_budget: 50000,
    }
    .execute(context)
    .await?;

    // Fetch gas again
    SuiClientCommands::Gas {
        address: Some(address),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address1 = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let address2 = SuiAddress::random_for_testing_only();

    // publish the object basics package
    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address1)
        .await?;
    let gas_obj_id = object_refs.first().unwrap().object_id;
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("move_call_args_linter");
    let build_config = BuildConfig::default();
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        gas: Some(gas_obj_id),
        gas_budget: 20_000,
        verify_dependencies: true,
    }
    .execute(context)
    .await?;

    let package = if let SuiClientCommandResult::Publish(response) = resp {
        response.effects.created[0].reference.object_id
    } else {
        unreachable!("Invalid response");
    };

    // Print objects owned by `address1`
    SuiClientCommands::Objects {
        address: Some(address1),
    }
    .execute(context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address1)
        .await?;

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let gas = object_refs.first().unwrap().object_id;
    let obj = object_refs.get(1).unwrap().object_id;

    // Create the args
    let args = vec![
        SuiJsonValue::new(json!(123u8))?,
        SuiJsonValue::new(json!(address1))?,
    ];

    // Test case with no gas specified
    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        gas: None,
        gas_budget: 20_000,
    }
    .execute(context)
    .await?;
    resp.print(true);

    // Get the created object
    let created_obj: ObjectID = if let SuiClientCommandResult::Call(
        _,
        SuiTransactionEffects {
            created: new_objs, ..
        },
    ) = resp
    {
        new_objs.first().unwrap().reference.object_id
    } else {
        // User assert since panic causes test issues
        assert!(false);
        // Use this to satisfy type checker
        ObjectID::random()
    };

    // Try a bad argument: decimal
    let args_json = json!([0.3f32, address1]);
    assert!(SuiJsonValue::new(args_json.as_array().unwrap().get(0).unwrap().clone()).is_err());

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
        gas: Some(gas),
        gas_budget: 20_000,
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    let err_string = format!("{} ", resp.err().unwrap());
    assert!(err_string.contains("Expected 2 args, found 1"));

    // Try a transfer
    // This should fail due to mismatch of object being sent
    let args = vec![
        SuiJsonValue::new(json!(obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        gas: Some(gas),
        gas_budget: 20_000,
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    let err_string = format!("{} ", resp.err().unwrap());
    let framework_addr = SUI_FRAMEWORK_ADDRESS.to_hex_literal();
    let package_addr = package.to_hex_literal();
    assert!(err_string.contains(&format!("Expected argument of type {package_addr}::object_basics::Object, but found type {framework_addr}::coin::Coin<{framework_addr}::sui::SUI>")));

    // Try a proper transfer
    let args = vec![
        SuiJsonValue::new(json!(created_obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        gas: Some(gas),
        gas_budget: 20_000,
    }
    .execute(context)
    .await?;

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let build_config = BuildConfig::default();
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        gas: Some(gas_obj_id),
        gas_budget: 20_000,
        verify_dependencies: true,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let obj_ids = if let SuiClientCommandResult::Publish(response) = resp {
        response
            .effects
            .created
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

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let recipient = SuiAddress::random_for_testing_only();

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object_id;
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        gas: Some(gas_obj_id),
        to: recipient,
        object_id: obj_id,
        gas_budget: 50000,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (mut_obj1, mut_obj2) =
        if let SuiClientCommandResult::Transfer(_, _, SuiTransactionEffects { mutated, .. }) = resp
        {
            (
                mutated.get(0).unwrap().reference.object_id,
                mutated.get(1).unwrap().reference.object_id,
            )
        } else {
            assert!(false);
            panic!()
        };

    // Check the objects
    let resp = SuiClientCommands::Object { id: mut_obj1 }
        .execute(context)
        .await?;
    let mut_obj1 =
        if let SuiClientCommandResult::Object(GetObjectDataResponse::Exists(object)) = resp {
            object
        } else {
            // Fail this way because Panic! causes test issues
            assert!(false);
            panic!()
        };

    let resp = SuiClientCommands::Object { id: mut_obj2 }
        .execute(context)
        .await?;
    let mut_obj2 =
        if let SuiClientCommandResult::Object(GetObjectDataResponse::Exists(object)) = resp {
            object
        } else {
            // Fail this way because Panic! causes test issues
            assert!(false);
            panic!()
        };

    let (gas, obj) = if mut_obj1.owner.get_owner_address().unwrap() == address {
        (mut_obj1, mut_obj2)
    } else {
        (mut_obj2, mut_obj1)
    };

    assert_eq!(gas.owner.get_owner_address().unwrap(), address);
    assert_eq!(obj.owner.get_owner_address().unwrap(), recipient);

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        gas: None,
        to: recipient,
        object_id: obj_id,
        gas_budget: 50000,
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (_mut_obj1, _mut_obj2) =
        if let SuiClientCommandResult::Transfer(_, _, SuiTransactionEffects { mutated, .. }) = resp
        {
            (
                mutated.get(0).unwrap().reference.object_id,
                mutated.get(1).unwrap().reference.object_id,
            )
        } else {
            assert!(false);
            panic!()
        };

    Ok(())
}

#[test]
// Test for issue https://github.com/MystenLabs/sui/issues/1078
fn test_bug_1078() {
    let read = SuiClientCommandResult::Object(GetObjectDataResponse::NotExists(ObjectID::random()));
    let mut writer = String::new();
    // fmt ObjectRead should not fail.
    write!(writer, "{}", read).unwrap();
    write!(writer, "{:?}", read).unwrap();
}

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_switch_command() -> Result<(), anyhow::Error> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
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
    let mut actual_objs = context
        .client
        .read_api()
        .get_objects_owned_by_address(addr1)
        .await
        .unwrap();
    cmd_objs.sort();
    actual_objs.sort();
    assert_eq!(cmd_objs, actual_objs);

    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(addr2),
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
                address: Some(addr2),
                env: None
            })
        )
    );

    // Wipe all the address info
    context.config.active_address = None;

    // Create a new address
    let os = SuiClientCommands::NewAddress {
        key_scheme: SignatureScheme::ED25519,
        derivation_path: None,
    }
    .execute(context)
    .await?;
    let new_addr = if let SuiClientCommandResult::NewAddress((a, _, _)) = os {
        a
    } else {
        panic!("Command failed")
    };

    // Check that we can switch to this address
    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(new_addr),
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
                address: Some(new_addr),
                env: None
            })
        )
    );
    Ok(())
}

#[sim_test]
async fn test_new_address_command_by_flag() -> Result<(), anyhow::Error> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
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
        derivation_path: None,
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

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_active_address_command() -> Result<(), anyhow::Error> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
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
        address: Some(addr2),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2),
                env: None
            })
        )
    );
    Ok(())
}

fn get_gas_value(o: &SuiParsedObject) -> u64 {
    GasCoin::try_from(o).unwrap().value()
}

async fn get_object(id: ObjectID, context: &WalletContext) -> Option<SuiParsedObject> {
    let response = context
        .client
        .read_api()
        .get_parsed_object(id)
        .await
        .unwrap();
    if let GetObjectDataResponse::Exists(o) = response {
        Some(o)
    } else {
        None
    }
}

async fn get_parsed_object_assert_existence(
    object_id: ObjectID,
    context: &WalletContext,
) -> SuiObject<SuiParsedData> {
    get_object(object_id, context)
        .await
        .expect("Object {object_id} does not exist.")
}

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_merge_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: Some(gas),
        gas_budget: 20_000,
    }
    .execute(context)
    .await?;
    let g = if let SuiClientCommandResult::MergeCoin(r) = resp {
        let object_id = r
            .effects
            .mutated_excluding_gas()
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

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: None,
        gas_budget: 10_000,
    }
    .execute(context)
    .await?;

    let g = if let SuiClientCommandResult::MergeCoin(r) = resp {
        let object_id = r
            .effects
            .mutated_excluding_gas()
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

#[allow(clippy::assertions_on_constants)]
#[sim_test]
async fn test_split_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let mut coin = object_refs.get(1).unwrap().object_id;

    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::SplitCoin {
        gas: Some(gas),
        gas_budget: 20_000,
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::SplitCoin(r) = resp {
        let updated_object_id = r
            .effects
            .mutated_excluding_gas()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.created;
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

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Get another coin
    for c in object_refs {
        if get_gas_value(&get_object(c.object_id, context).await.unwrap()) > 2000 {
            coin = c.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test split coin into equal parts
    let resp = SuiClientCommands::SplitCoin {
        gas: None,
        gas_budget: 20_000,
        coin_id: coin,
        amounts: None,
        count: Some(3),
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::SplitCoin(r) = resp {
        let updated_object_id = r
            .effects
            .mutated_excluding_gas()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.created;
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

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Get another coin
    for c in object_refs {
        if get_gas_value(&get_object(c.object_id, context).await.unwrap()) > 2000 {
            coin = c.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::SplitCoin {
        gas: None,
        gas_budget: 20_000,
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::SplitCoin(r) = resp {
        let updated_object_id = r
            .effects
            .mutated_excluding_gas()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.created;
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
    assert!(res.is_err());

    let res = SignatureScheme::from_flag("something");
    assert!(res.is_err());
    Ok(())
}

#[sim_test]
async fn test_execute_signed_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut test_cluster.wallet;
    let mut txns = make_transactions_with_wallet_context(context, 1).await;
    let txn = txns.swap_remove(0);

    let (tx_data, scheme, signature, pubkey) = txn.to_network_data_for_execution();
    SuiClientCommands::ExecuteSignedTx {
        tx_data: tx_data.encoded(),
        scheme,
        pubkey: pubkey.encoded(),
        signature: signature.encoded(),
    }
    .execute(context)
    .await?;
    Ok(())
}

#[sim_test]
async fn test_serialize_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = init_cluster_builder_env_aware().build().await?;
    let address = test_cluster.get_address_0();
    let address1 = test_cluster.get_address_1();
    let context = &mut test_cluster.wallet;

    let object_refs = context
        .client
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;
    let coin = object_refs.get(1).unwrap().object_id;

    SuiClientCommands::SerializeTransferSui {
        to: address1,
        sui_coin_object_id: coin,
        gas_budget: 1000,
        amount: Some(1),
    }
    .execute(context)
    .await?;
    Ok(())
}
