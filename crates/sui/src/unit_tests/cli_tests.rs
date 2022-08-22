// Copyright (c) 2022, Mysten Labs, Inc.
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
use sui_config::gateway::GatewayConfig;
use sui_config::genesis_config::{AccountConfig, GenesisConfig, ObjectConfig};
use sui_config::{
    Config, NetworkConfig, PersistedConfig, ValidatorInfo, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG,
    SUI_GATEWAY_CONFIG, SUI_GENESIS_FILENAME, SUI_KEYSTORE_FILENAME, SUI_NETWORK_CONFIG,
};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{GetObjectDataResponse, SuiData, SuiParsedObject, SuiTransactionEffects};
use sui_sdk::crypto::KeystoreType;
use sui_sdk::ClientType;
use sui_types::crypto::{
    generate_proof_of_possession, AccountKeyPair, AuthorityKeyPair, KeypairTraits, SuiKeyPair,
};
use sui_types::{base_types::ObjectID, crypto::get_key_pair, gas_coin::GasCoin};
use sui_types::{sui_framework_address_concat_string, SUI_FRAMEWORK_ADDRESS};

use test_utils::network::{setup_network_and_wallet, start_test_network};

const TEST_DATA_DIR: &str = "src/unit_tests/data/";

#[tokio::test]
async fn test_genesis() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    let config = working_dir.join(SUI_NETWORK_CONFIG);

    // Start network without authorities
    let start = SuiCommand::Start {
        config: Some(config),
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

    assert_eq!(10, files.len());
    assert!(files.contains(&SUI_CLIENT_CONFIG.to_string()));
    assert!(files.contains(&SUI_GATEWAY_CONFIG.to_string()));
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

    if let ClientType::Embedded(config) = &wallet_conf.gateway {
        assert_eq!(4, config.validator_set.len());
        assert_eq!(working_dir.join("client_db"), config.db_folder_path);
    } else {
        panic!()
    }

    assert_eq!(5, wallet_conf.keystore.init().unwrap().addresses().len());

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
    let temp_dir = tempfile::tempdir().unwrap();
    let working_dir = temp_dir.path();
    let keypair: AuthorityKeyPair = get_key_pair().1;

    let wallet_config = SuiClientConfig {
        keystore: KeystoreType::File(working_dir.join(SUI_KEYSTORE_FILENAME)),
        gateway: ClientType::Embedded(GatewayConfig {
            db_folder_path: working_dir.join("client_db"),
            validator_set: vec![ValidatorInfo {
                name: "0".into(),
                public_key: keypair.public().into(),
                network_key: get_key_pair::<AccountKeyPair>().1.public().clone().into(),
                proof_of_possession: generate_proof_of_possession(&keypair),
                stake: 1,
                delegation: 1,
                gas_price: 1,
                network_address: sui_config::utils::new_network_address(),
                narwhal_primary_to_primary: sui_config::utils::new_network_address(),
                narwhal_worker_to_primary: sui_config::utils::new_network_address(),
                narwhal_primary_to_worker: sui_config::utils::new_network_address(),
                narwhal_worker_to_worker: sui_config::utils::new_network_address(),
                narwhal_consensus_address: sui_config::utils::new_network_address(),
            }],
            ..Default::default()
        }),
        active_address: None,
        fullnode: None,
    };
    let wallet_conf_path = working_dir.join(SUI_CLIENT_CONFIG);
    let wallet_config = wallet_config.persisted(&wallet_conf_path);
    wallet_config.save().unwrap();
    let mut context = WalletContext::new(&wallet_conf_path).await.unwrap();

    // Add 3 accounts
    for _ in 0..3 {
        context
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

#[tokio::test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let _object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_create_example_nft_command() {
    let (_network, mut context, address) = setup_network_and_wallet().await.unwrap();

    let result = SuiClientCommands::CreateExampleNFT {
        name: None,
        description: None,
        url: None,
        gas: None,
        gas_budget: None,
    }
    .execute(&mut context)
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

#[tokio::test]
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

    let network = start_test_network(Some(config)).await?;

    // Wallet config
    let mut context = WalletContext::new(&network.dir().join(SUI_CLIENT_CONFIG)).await?;
    assert_eq!(1, context.keystore.addresses().len());
    let address = context.keystore.addresses().first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    Ok(())
}

#[tokio::test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().object_id;

    SuiClientCommands::Object { id: object_id }
        .execute(&mut context)
        .await?
        .print(true);

    Ok(())
}

#[tokio::test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let recipient = context.keystore.addresses().get(1).cloned().unwrap();

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    let object_id = object_refs.first().unwrap().object_id;
    let object_to_send = object_refs.get(1).unwrap().object_id;

    SuiClientCommands::Gas {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send an object
    SuiClientCommands::Transfer {
        to: recipient,
        coin_object_id: object_to_send,
        gas: Some(object_id),
        gas_budget: 50000,
    }
    .execute(&mut context)
    .await?;

    // Fetch gas again
    SuiClientCommands::Gas {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address1) = setup_network_and_wallet().await?;
    let address2 = context.keystore.addresses().get(1).cloned().unwrap();

    // publish the object basics package
    let object_refs = context
        .gateway
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
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;
    let package = if let SuiClientCommandResult::Publish(response) = resp {
        let publish_resp = response.parsed_data.unwrap().to_publish_response().unwrap();
        publish_resp.package.object_id
    } else {
        unreachable!("Invalid response");
    };

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address2),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Print objects owned by `address1`
    SuiClientCommands::Objects {
        address: Some(address1),
    }
    .execute(&mut context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let object_refs = context
        .gateway
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
        gas_budget: 1000,
    }
    .execute(&mut context)
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
        gas_budget: 1000,
    }
    .execute(&mut context)
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
        gas_budget: 1000,
    }
    .execute(&mut context)
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
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
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
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let (package, created_obj) = if let SuiClientCommandResult::Publish(response) = resp {
        let publish_resp = response.parsed_data.unwrap().to_publish_response().unwrap();
        (
            publish_resp.package,
            publish_resp.created_objects[0].reference.clone(),
        )
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    let resp = SuiClientCommands::Object {
        id: package.object_id,
    }
    .execute(&mut context)
    .await?;
    assert!(matches!(
        resp,
        SuiClientCommandResult::Object(GetObjectDataResponse::Exists(..))
    ));

    let resp = SuiClientCommands::Object {
        id: created_obj.object_id,
    }
    .execute(&mut context)
    .await?;
    assert!(matches!(
        resp,
        SuiClientCommandResult::Object(GetObjectDataResponse::Exists(..))
    ));

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let recipient = context.keystore.addresses().get(1).cloned().unwrap();

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object_id;
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        gas: Some(gas_obj_id),
        to: recipient,
        coin_object_id: obj_id,
        gas_budget: 50000,
    }
    .execute(&mut context)
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

    // Sync both to fetch objects
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);
    SuiClientCommands::SyncClientState {
        address: Some(recipient),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Check the objects
    let resp = SuiClientCommands::Object { id: mut_obj1 }
        .execute(&mut context)
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
        .execute(&mut context)
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

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        gas: None,
        to: recipient,
        coin_object_id: obj_id,
        gas_budget: 50000,
    }
    .execute(&mut context)
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
#[tokio::test]
async fn test_switch_command() -> Result<(), anyhow::Error> {
    let network = start_test_network(None).await?;

    // Create Wallet context.
    let wallet_conf = network.dir().join(SUI_CLIENT_CONFIG);

    let mut context = WalletContext::new(&wallet_conf).await?;

    // Get the active address
    let addr1 = context.active_address()?;

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(addr1),
    }
    .execute(&mut context)
    .await?;

    // Run a command with address omitted
    let os = SuiClientCommands::Objects { address: None }
        .execute(&mut context)
        .await?;

    let mut cmd_objs = if let SuiClientCommandResult::Objects(v) = os {
        v
    } else {
        panic!("Command failed")
    };

    // Check that we indeed fetched for addr1
    let mut actual_objs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(addr1)
        .await
        .unwrap();
    cmd_objs.sort();
    actual_objs.sort();
    assert_eq!(cmd_objs, actual_objs);

    // Switch the address
    let addr2 = context.keystore.addresses().get(1).cloned().unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(addr2),
        gateway: None,
        fullnode: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(addr2, context.active_address()?);
    assert_ne!(addr1, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2),
                gateway: None,
                fullnode: None,
            })
        )
    );

    // Wipe all the address info
    context.config.active_address = None;

    // Create a new address
    let os = SuiClientCommands::NewAddress {}
        .execute(&mut context)
        .await?;
    let new_addr = if let SuiClientCommandResult::NewAddress((a, _)) = os {
        a
    } else {
        panic!("Command failed")
    };

    // Check that we can switch to this address
    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(new_addr),
        gateway: None,
        fullnode: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(new_addr, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(new_addr),
                gateway: None,
                fullnode: None,
            })
        )
    );
    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_active_address_command() -> Result<(), anyhow::Error> {
    let network = start_test_network(None).await?;

    // Create Wallet context.
    let wallet_conf = network.dir().join(SUI_CLIENT_CONFIG);

    let mut context = WalletContext::new(&wallet_conf).await?;

    // Get the active address
    let addr1 = context.active_address()?;

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(addr1),
    }
    .execute(&mut context)
    .await?;

    // Run a command with address omitted
    let os = SuiClientCommands::ActiveAddress {}
        .execute(&mut context)
        .await?;

    let a = if let SuiClientCommandResult::ActiveAddress(Some(v)) = os {
        v
    } else {
        panic!("Command failed")
    };
    assert_eq!(a, addr1);

    let addr2 = context.keystore.addresses().get(1).cloned().unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(addr2),
        gateway: None,
        fullnode: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2),
                gateway: None,
                fullnode: None
            })
        )
    );
    Ok(())
}

fn get_gas_value(o: &SuiParsedObject) -> u64 {
    GasCoin::try_from(o).unwrap().value()
}

async fn get_object(id: ObjectID, context: &mut WalletContext) -> Option<SuiParsedObject> {
    let response = context
        .gateway
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

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_merge_coin() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, &mut context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, &mut context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: Some(gas),
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    let g = if let SuiClientCommandResult::MergeCoin(r) = resp {
        r.parsed_data.unwrap().to_merge_coin_response().unwrap()
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g.updated_coin), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, &mut context).await, None);

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?;
    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, &mut context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, &mut context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: None,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    let g = if let SuiClientCommandResult::MergeCoin(r) = resp {
        r.parsed_data.unwrap().to_merge_coin_response().unwrap()
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g.updated_coin), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, &mut context).await, None);

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[tokio::test]
async fn test_split_coin() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let mut coin = object_refs.get(1).unwrap().object_id;

    let orig_value = get_gas_value(&get_object(coin, &mut context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::SplitCoin {
        gas: Some(gas),
        gas_budget: 1000,
        coin_id: coin,
        amounts: vec![1000, 10],
    }
    .execute(&mut context)
    .await?;

    let g = if let SuiClientCommandResult::SplitCoin(r) = resp {
        r.parsed_data.unwrap().to_split_coin_response().unwrap()
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&g.updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&g.new_coins[0]) == 1000) || (get_gas_value(&g.new_coins[0]) == 10));
    assert!((get_gas_value(&g.new_coins[1]) == 1000) || (get_gas_value(&g.new_coins[1]) == 10));

    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Get another coin
    for c in object_refs {
        if get_gas_value(&get_object(c.object_id, &mut context).await.unwrap()) > 2000 {
            coin = c.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, &mut context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::SplitCoin {
        gas: None,
        gas_budget: 1000,
        coin_id: coin,
        amounts: vec![1000, 10],
    }
    .execute(&mut context)
    .await?;

    let g = if let SuiClientCommandResult::SplitCoin(r) = resp {
        r.parsed_data.unwrap().to_split_coin_response().unwrap()
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&g.updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&g.new_coins[0]) == 1000) || (get_gas_value(&g.new_coins[0]) == 10));
    assert!((get_gas_value(&g.new_coins[1]) == 1000) || (get_gas_value(&g.new_coins[1]) == 10));
    Ok(())
}
