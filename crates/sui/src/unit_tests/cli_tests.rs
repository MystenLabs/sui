// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Write, fs::read_dir, ops::Add, path::PathBuf, str, time::Duration};

use anyhow::anyhow;
use serde_json::{json, Value};
use tracing_test::traced_test;

use sui::wallet_commands::SwitchResponse;
use sui::{
    config::{
        Config, GatewayConfig, GatewayType, PersistedConfig, WalletConfig, SUI_GATEWAY_CONFIG,
        SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG,
    },
    keystore::KeystoreType,
    sui_commands::SuiCommand,
    wallet_commands::{WalletCommandResult, WalletCommands, WalletContext},
};
use sui_config::{AccountConfig, GenesisConfig, NetworkConfig, ObjectConfig, SUI_FULLNODE_CONFIG};
use sui_core::gateway_types::{GetObjectInfoResponse, SuiObject, SuiTransactionEffects};
use sui_json::SuiJsonValue;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    crypto::get_key_pair,
    gas_coin::GasCoin,
    object::GAS_VALUE_FOR_TESTING,
};

use test_utils::network::{setup_network_and_wallet, start_test_network};

const TEST_DATA_DIR: &str = "src/unit_tests/data/";

macro_rules! retry_assert {
    ($test:expr, $timeout:expr) => {{
        let mut duration = Duration::from_secs(0);
        let max_duration: Duration = $timeout;
        let sleep_duration = Duration::from_millis(100);
        while duration.lt(&max_duration) && !$test {
            tokio::time::sleep(sleep_duration).await;
            duration = duration.add(sleep_duration);
        }
        assert!(duration.lt(&max_duration));
    }};
}

#[traced_test]
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
    assert!(logs_contain("Network genesis completed."));

    // Get all the new file names
    let files = read_dir(working_dir)?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(9, files.len());
    assert!(files.contains(&SUI_WALLET_CONFIG.to_string()));
    assert!(files.contains(&SUI_GATEWAY_CONFIG.to_string()));
    assert!(files.contains(&SUI_NETWORK_CONFIG.to_string()));
    assert!(files.contains(&SUI_FULLNODE_CONFIG.to_string()));

    assert!(files.contains(&"wallet.key".to_string()));

    // Check network config
    let network_conf =
        PersistedConfig::<NetworkConfig>::read(&working_dir.join(SUI_NETWORK_CONFIG))?;
    assert_eq!(4, network_conf.validator_configs().len());

    // Check wallet config
    let wallet_conf = PersistedConfig::<WalletConfig>::read(&working_dir.join(SUI_WALLET_CONFIG))?;

    if let GatewayType::Embedded(config) = &wallet_conf.gateway {
        assert_eq!(4, config.validator_set.len());
        assert_eq!(working_dir.join("client_db"), config.db_folder_path);
    } else {
        panic!()
    }

    assert_eq!(5, wallet_conf.accounts.len());

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

#[traced_test]
#[tokio::test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();

    let wallet_config = WalletConfig {
        accounts: vec![],
        keystore: KeystoreType::File(working_dir.join("wallet.key")),
        gateway: GatewayType::Embedded(GatewayConfig {
            db_folder_path: working_dir.join("client_db"),
            ..Default::default()
        }),
        active_address: None,
    };
    let wallet_conf_path = working_dir.join(SUI_WALLET_CONFIG);
    let mut wallet_config = wallet_config.persisted(&wallet_conf_path);

    // Add 3 accounts
    for _ in 0..3 {
        wallet_config.accounts.push({
            let (address, _) = get_key_pair();
            address
        });
    }
    wallet_config.save()?;

    let mut context = WalletContext::new(&wallet_conf_path)?;

    // Print all addresses
    WalletCommands::Addresses
        .execute(&mut context)
        .await?
        .print(true);

    // Check log output contains all addresses
    for address in &context.config.accounts {
        assert!(logs_contain(&*format!("{address}")));
    }

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    // Print objects owned by `address`
    WalletCommands::Objects {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    for oref in object_refs {
        assert!(logs_contain(format!("{}", oref.object_id).as_str()))
    }

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_create_example_nft_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let result = WalletCommands::CreateExampleNFT {
        name: None,
        description: None,
        url: None,
        gas: None,
        gas_budget: None,
    }
    .execute(&mut context)
    .await?;

    match result {
        WalletCommandResult::CreateExampleNFT(GetObjectDataResponse::Exists(obj)) => {
            assert_eq!(obj.owner, address);
            assert_eq!(obj.data.type_().unwrap(), "0x2::DevNetNFT::DevNetNFT");
            Ok(obj)
        }
        _ => Err(anyhow!(
            "WalletCommands::CreateExampleNFT returns wrong type"
        )),
    }?;

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_custom_genesis() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    // Create and save genesis config file
    // Create 4 authorities, 1 account with 1 gas object with custom id

    let mut config = GenesisConfig::for_local_testing()?;
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

    let _network = start_test_network(working_dir.path(), Some(config)).await?;

    // Wallet config
    let mut context = WalletContext::new(&working_dir.path().join(SUI_WALLET_CONFIG))?;
    assert_eq!(1, context.config.accounts.len());
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Print objects owned by `address`
    WalletCommands::Objects {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // confirm the object with custom object id.
    retry_assert!(
        logs_contain(format!("{object_id}").as_str()),
        Duration::from_millis(5000)
    );

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_custom_genesis_with_custom_move_package() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    // Create and save genesis config file
    // Create 4 authorities and 1 account
    let num_authorities = 4;
    let mut config = GenesisConfig::custom_genesis(num_authorities, 1, 1)?;
    config
        .move_packages
        .push(PathBuf::from(TEST_DATA_DIR).join("custom_genesis_package_1"));
    config
        .move_packages
        .push(PathBuf::from(TEST_DATA_DIR).join("custom_genesis_package_2"));

    // Start network
    let _network = start_test_network(working_dir, Some(config)).await?;

    assert!(logs_contain("Loading 2 Move packages"));
    // Checks network config contains package ids
    let network_conf =
        PersistedConfig::<NetworkConfig>::read(&working_dir.join(SUI_NETWORK_CONFIG))?;
    // assert_eq!(2, network_conf.loaded_move_packages().len());

    // Make sure we log out package id
    for (_, id) in network_conf.loaded_move_packages() {
        assert!(logs_contain(&*format!("{id}")));
    }

    // Create Wallet context.
    let wallet_conf_path = working_dir.join(SUI_WALLET_CONFIG);
    let mut context = WalletContext::new(&wallet_conf_path)?;

    // Make sure init() is executed correctly for custom_genesis_package_2::M1
    let move_objects =
        get_move_objects_by_type(&mut context, SuiAddress::default(), "M1::Object").await?;
    assert_eq!(move_objects.len(), 1);
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().object_id;

    WalletCommands::Object { id: object_id }
        .execute(&mut context)
        .await?
        .print(true);
    let obj_owner = format!("{}", address);

    retry_assert!(
        logs_contain(obj_owner.as_str()),
        Duration::from_millis(5000)
    );

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let recipient = context.config.accounts.get(1).cloned().unwrap();

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    let object_id = object_refs.first().unwrap().object_id;
    let object_to_send = object_refs.get(1).unwrap().object_id;

    WalletCommands::Gas {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);
    let object_id_str = format!("{object_id}");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that the value got printed
    logs_assert(|lines: &[&str]| {
        let matches = lines
            .iter()
            .filter_map(|line| {
                if line.contains(&object_id_str) {
                    return extract_gas_info(line);
                }
                None
            })
            .collect::<Vec<_>>();

        assert_eq!(matches.len(), 1);

        // Extract the values
        let (obj_id, version, val) = *matches.get(0).unwrap();

        assert_eq!(obj_id, object_id);
        assert_eq!(version, SequenceNumber::new());
        assert_eq!(val, GAS_VALUE_FOR_TESTING);

        Ok(())
    });

    // Send an object
    WalletCommands::Transfer {
        to: recipient,
        coin_object_id: object_to_send,
        gas: Some(object_id),
        gas_budget: 50000,
    }
    .execute(&mut context)
    .await?;

    // Fetch gas again
    WalletCommands::Gas {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that the value got printed and updated
    logs_assert(|lines: &[&str]| {
        let matches = lines
            .iter()
            .filter_map(|line| {
                if line.contains(&object_id_str) {
                    return extract_gas_info(line);
                }
                None
            })
            .collect::<Vec<_>>();

        assert_eq!(matches.len(), 2);

        // Extract the values
        let (obj_id, version, val) = *matches.get(1).unwrap();

        assert_eq!(obj_id, object_id);
        assert_eq!(version, SequenceNumber::from_u64(1));
        assert!(val < GAS_VALUE_FOR_TESTING);

        Ok(())
    });

    Ok(())
}

fn extract_gas_info(s: &str) -> Option<(ObjectID, SequenceNumber, u64)> {
    let tokens = s.split('|').map(|q| q.trim()).collect::<Vec<_>>();
    if tokens.len() != 3 {
        return None;
    }

    let id_str = tokens[0]
        .split(':')
        .map(|q| q.trim())
        .collect::<Vec<_>>()
        .iter()
        .last()
        .unwrap()
        .to_owned();
    Some((
        ObjectID::from_hex_literal(id_str).unwrap(),
        SequenceNumber::from_u64(tokens[1].parse::<u64>().unwrap()),
        tokens[2].parse::<u64>().unwrap(),
    ))
}

async fn get_move_objects_by_type(
    context: &mut WalletContext,
    address: SuiAddress,
    type_substr: &str,
) -> Result<Vec<(ObjectID, Value)>, anyhow::Error> {
    let objects = get_move_objects(context, address).await?;
    Ok(objects
        .into_iter()
        .filter(|(_, obj)| obj["data"]["type"].to_string().contains(type_substr))
        .collect())
}

async fn get_move_objects(
    context: &mut WalletContext,
    address: SuiAddress,
) -> Result<Vec<(ObjectID, Value)>, anyhow::Error> {
    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(context)
    .await?
    .print(true);

    // Fetch objects owned by `address`
    let objects_result = WalletCommands::Objects {
        address: Some(address),
    }
    .execute(context)
    .await?;

    match objects_result {
        WalletCommandResult::Objects(object_refs) => {
            let mut objs = vec![];
            for oref in object_refs {
                objs.push((
                    oref.object_id,
                    get_move_object(context, oref.object_id).await?,
                ));
            }
            Ok(objs)
        }
        _ => panic!(
            "WalletCommands::Objects returns wrong type {}",
            objects_result
        ),
    }
}

async fn get_move_object(
    context: &mut WalletContext,
    id: ObjectID,
) -> Result<Value, anyhow::Error> {
    let obj = WalletCommands::Object { id }.execute(context).await?;

    match obj {
        WalletCommandResult::Object(obj) => match obj {
            GetObjectDataResponse::Exists(obj) => Ok(serde_json::to_value(obj)?),
            _ => panic!("WalletCommands::Object returns wrong type"),
        },
        _ => panic!("WalletCommands::Object returns wrong type {obj}"),
    }
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address1) = setup_network_and_wallet().await?;
    let address2 = context.config.accounts.get(1).cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address2),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Print objects owned by `address1`
    WalletCommands::Objects {
        address: Some(address1),
    }
    .execute(&mut context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address1)
        .await?;

    // Check log output contains all object ids.
    for oref in &object_refs {
        assert!(logs_contain(format!("{}", oref.object_id).as_str()))
    }

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let gas = object_refs.first().unwrap().object_id;
    let obj = object_refs.get(1).unwrap().object_id;

    // Create the args
    let addr1_str = format!("0x{:02x}", address1);
    let args_json = json!([123u8, addr1_str]);

    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    // Test case with no gas specified
    let resp = WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: "ObjectBasics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        gas: None,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;
    resp.print(true);

    retry_assert!(
        logs_contain("Mutated Objects:"),
        Duration::from_millis(1000)
    );
    assert!(logs_contain("Created Objects:"));

    // Get the created object
    let created_obj: ObjectID = if let WalletCommandResult::Call(
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
    let args_json = json!([0.3f32, addr1_str]);
    assert!(SuiJsonValue::new(args_json.as_array().unwrap().get(0).unwrap().clone()).is_err());

    // Try a bad argument: too few args
    let args_json = json!([300usize]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    let resp = WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: "ObjectBasics".to_string(),
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
    let obj_str = format!("0x{:02x}", obj);
    let addr2_str = format!("0x{:02x}", address2);

    let args_json = json!([obj_str, addr2_str]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    let resp = WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: "ObjectBasics".to_string(),
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
    assert!(err_string.contains("Expected argument of type 0x2::ObjectBasics::Object, but found type 0x2::Coin::Coin<0x2::SUI::SUI>"));

    // Try a proper transfer
    let obj_str = format!("0x{:02x}", created_obj);
    let addr2_str = format!("0x{:02x}", address2);

    let args_json = json!([obj_str, addr2_str]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: "ObjectBasics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        gas: Some(gas),
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    retry_assert!(
        logs_contain("Mutated Objects:"),
        Duration::from_millis(1000)
    );
    assert!(logs_contain("Created Objects:"));

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object_id;

    // Provide path to well formed package sources
    let mut path = TEST_DATA_DIR.to_owned();
    path.push_str("dummy_modules_publish");

    let resp = WalletCommands::Publish {
        path,
        gas: Some(gas_obj_id),
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let (package, created_obj) = if let WalletCommandResult::Publish(response) = resp {
        (
            response.package,
            response.created_objects[0].reference.clone(),
        )
    } else {
        unreachable!("Invalid response");
    };

    // One is the actual module, while the other is the object created at init
    retry_assert!(
        logs_contain(&format!("{}", package.object_id)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{}", created_obj.object_id)),
        Duration::from_millis(5000)
    );

    // Check the objects
    let resp = WalletCommands::Object {
        id: package.object_id,
    }
    .execute(&mut context)
    .await?;
    assert!(matches!(
        resp,
        WalletCommandResult::Object(GetObjectDataResponse::Exists(..))
    ));

    let resp = WalletCommands::Object {
        id: created_obj.object_id,
    }
    .execute(&mut context)
    .await?;
    assert!(matches!(
        resp,
        WalletCommandResult::Object(GetObjectDataResponse::Exists(..))
    ));

    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let recipient = context.config.accounts.get(1).cloned().unwrap();

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object_id;
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = WalletCommands::Transfer {
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
        if let WalletCommandResult::Transfer(_, _, SuiTransactionEffects { mutated, .. }) = resp {
            (
                mutated.get(0).unwrap().reference.object_id,
                mutated.get(1).unwrap().reference.object_id,
            )
        } else {
            assert!(false);
            panic!()
        };

    retry_assert!(
        logs_contain(&format!("{}", mut_obj1)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{}", mut_obj2)),
        Duration::from_millis(5000)
    );

    // Sync both to fetch objects
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);
    WalletCommands::SyncClientState {
        address: Some(recipient),
    }
    .execute(&mut context)
    .await?
    .print(true);

    // Check the objects
    let resp = WalletCommands::Object { id: mut_obj1 }
        .execute(&mut context)
        .await?;
    let mut_obj1 = if let WalletCommandResult::Object(GetObjectDataResponse::Exists(object)) = resp
    {
        object
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false);
        panic!()
    };

    let resp = WalletCommands::Object { id: mut_obj2 }
        .execute(&mut context)
        .await?;
    let mut_obj2 = if let WalletCommandResult::Object(GetObjectDataResponse::Exists(object)) = resp
    {
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
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = WalletCommands::Transfer {
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
    let (mut_obj1, mut_obj2) =
        if let WalletCommandResult::Transfer(_, _, SuiTransactionEffects { mutated, .. }) = resp {
            (
                mutated.get(0).unwrap().reference.object_id,
                mutated.get(1).unwrap().reference.object_id,
            )
        } else {
            assert!(false);
            panic!()
        };

    retry_assert!(
        logs_contain(&format!("{}", mut_obj1)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{}", mut_obj2)),
        Duration::from_millis(5000)
    );

    Ok(())
}

#[test]
// Test for issue https://github.com/MystenLabs/sui/issues/1078
fn test_bug_1078() {
    let read = WalletCommandResult::Object(GetObjectDataResponse::NotExists(ObjectID::random()));
    let mut writer = String::new();
    // fmt ObjectRead should not fail.
    write!(writer, "{}", read).unwrap();
    write!(writer, "{:?}", read).unwrap();
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_switch_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let _network = start_test_network(working_dir.path(), None).await?;

    // Create Wallet context.
    let wallet_conf = working_dir.path().join(SUI_WALLET_CONFIG);

    let mut context = WalletContext::new(&wallet_conf)?;

    // Get the active address
    let addr1 = context.active_address()?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(addr1),
    }
    .execute(&mut context)
    .await?;

    // Run a command with address omitted
    let os = WalletCommands::Objects { address: None }
        .execute(&mut context)
        .await?;

    let mut cmd_objs = if let WalletCommandResult::Objects(v) = os {
        v
    } else {
        panic!("Command failed")
    };

    // Check that we indeed fetched for addr1
    let mut actual_objs = context
        .gateway
        .get_objects_owned_by_address(addr1)
        .await
        .unwrap();
    cmd_objs.sort();
    actual_objs.sort();
    assert_eq!(cmd_objs, actual_objs);

    // Switch the address
    let addr2 = context.config.accounts.get(1).cloned().unwrap();
    let resp = WalletCommands::Switch {
        address: Some(addr2),
        gateway: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(addr2, context.active_address()?);
    assert_ne!(addr1, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            WalletCommandResult::Switch(SwitchResponse {
                address: Some(addr2),
                gateway: None
            })
        )
    );

    // Wipe all the address info
    context.config.accounts.clear();
    context.config.active_address = None;

    // Create a new address
    let os = WalletCommands::NewAddress {}.execute(&mut context).await?;
    let new_addr = if let WalletCommandResult::NewAddress(a) = os {
        a
    } else {
        panic!("Command failed")
    };

    // Check that we can switch to this address
    // Switch the address
    let resp = WalletCommands::Switch {
        address: Some(new_addr),
        gateway: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(new_addr, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            WalletCommandResult::Switch(SwitchResponse {
                address: Some(new_addr),
                gateway: None
            })
        )
    );
    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_active_address_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let _network = start_test_network(working_dir.path(), None).await?;

    // Create Wallet context.
    let wallet_conf = working_dir.path().join(SUI_WALLET_CONFIG);

    let mut context = WalletContext::new(&wallet_conf)?;

    // Get the active address
    let addr1 = context.active_address()?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(addr1),
    }
    .execute(&mut context)
    .await?;

    // Run a command with address omitted
    let os = WalletCommands::ActiveAddress {}
        .execute(&mut context)
        .await?;

    let a = if let WalletCommandResult::ActiveAddress(Some(v)) = os {
        v
    } else {
        panic!("Command failed")
    };
    assert_eq!(a, addr1);

    let addr2 = context.config.accounts.get(1).cloned().unwrap();
    let resp = WalletCommands::Switch {
        address: Some(addr2),
        gateway: None,
    }
    .execute(&mut context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            WalletCommandResult::Switch(SwitchResponse {
                address: Some(addr2),
                gateway: None
            })
        )
    );
    Ok(())
}

fn get_gas_value(o: &SuiObject) -> u64 {
    GasCoin::try_from(o).unwrap().value()
}

async fn get_object(id: ObjectID, context: &mut WalletContext) -> Option<SuiObject> {
    if let GetObjectDataResponse::Exists(o) = context.gateway.get_object(id).await.unwrap() {
        Some(o)
    } else {
        None
    }
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_merge_coin() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, &mut context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, &mut context).await.unwrap());

    // Test with gas specified
    let resp = WalletCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: Some(gas),
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    let g = if let WalletCommandResult::MergeCoin(r) = resp {
        r
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g.updated_coin), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, &mut context).await, None);

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?;
    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, &mut context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, &mut context).await.unwrap());

    // Test with no gas specified
    let resp = WalletCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: None,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    let g = if let WalletCommandResult::MergeCoin(r) = resp {
        r
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
#[traced_test]
#[tokio::test]
async fn test_split_coin() -> Result<(), anyhow::Error> {
    let (_network, mut context, address) = setup_network_and_wallet().await?;
    let object_refs = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object_id;
    let mut coin = object_refs.get(1).unwrap().object_id;

    let orig_value = get_gas_value(&get_object(coin, &mut context).await.unwrap());

    // Test with gas specified
    let resp = WalletCommands::SplitCoin {
        gas: Some(gas),
        gas_budget: 1000,
        coin_id: coin,
        amounts: vec![1000, 10],
    }
    .execute(&mut context)
    .await?;

    let g = if let WalletCommandResult::SplitCoin(r) = resp {
        r
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&g.updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&g.new_coins[0]) == 1000) || (get_gas_value(&g.new_coins[0]) == 10));
    assert!((get_gas_value(&g.new_coins[1]) == 1000) || (get_gas_value(&g.new_coins[1]) == 10));

    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?
    .print(true);

    let object_refs = context
        .gateway
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
    let resp = WalletCommands::SplitCoin {
        gas: None,
        gas_budget: 1000,
        coin_id: coin,
        amounts: vec![1000, 10],
    }
    .execute(&mut context)
    .await?;

    let g = if let WalletCommandResult::SplitCoin(r) = resp {
        r
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&g.updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&g.new_coins[0]) == 1000) || (get_gas_value(&g.new_coins[0]) == 10));
    assert!((get_gas_value(&g.new_coins[1]) == 1000) || (get_gas_value(&g.new_coins[1]) == 10));
    Ok(())
}
