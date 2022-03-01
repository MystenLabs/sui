// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use move_core_types::identifier::Identifier;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs::read_dir;
use std::ops::Add;
use std::path::Path;
use std::time::Duration;
use sui::config::{
    AccountConfig, AccountInfo, AuthorityPrivateInfo, GenesisConfig, NetworkConfig, ObjectConfig,
    WalletConfig, AUTHORITIES_DB_NAME,
};
use sui::sui_json::SuiJsonValue;
use sui::wallet_commands::{WalletCommandResult, WalletCommands, WalletContext};
use sui_core::client::Client;
use sui_network::network::PortAllocator;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::messages::TransactionEffects;
use sui_types::object::{Data, MoveObject, Object, ObjectRead, GAS_VALUE_FOR_TESTING};
use tokio::task;
use tokio::task::JoinHandle;
use tracing_test::traced_test;

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
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Start network without authorities
    let start = SuiCommand::Start.execute(&mut config).await;
    assert!(matches!(start, Err(..)));
    // Genesis
    SuiCommand::Genesis { config: None }
        .execute(&mut config)
        .await?;
    assert!(logs_contain("Network genesis completed."));

    // Get all the new file names
    let files = read_dir(working_dir.path())?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(3, files.len());
    assert!(files.contains(&"wallet.conf".to_string()));
    assert!(files.contains(&AUTHORITIES_DB_NAME.to_string()));
    assert!(files.contains(&"network.conf".to_string()));

    // Check network.conf
    let network_conf = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;
    assert_eq!(4, network_conf.authorities.len());

    // Check wallet.conf
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    assert_eq!(4, wallet_conf.authorities.len());
    assert_eq!(5, wallet_conf.accounts.len());
    assert_eq!(
        working_dir.path().join("client_db"),
        wallet_conf.db_folder_path
    );

    working_dir.close()?;
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let mut wallet_config = WalletConfig::create(&working_dir.path().join("wallet.conf"))?;
    wallet_config.db_folder_path = working_dir.path().join("client_db");

    // Add 3 accounts
    for _ in 0..3 {
        wallet_config.accounts.push({
            let (address, key_pair) = get_key_pair();
            AccountInfo { address, key_pair }
        });
    }
    let mut context = WalletContext::new(wallet_config)?;

    // Print all addresses
    WalletCommands::Addresses
        .execute(&mut context)
        .await?
        .print(true);

    // Check log output contains all addresses
    for address in context.config.accounts.iter().map(|info| info.address) {
        assert!(logs_contain(&*format!("{}", address)));
    }

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let network = start_network(working_dir.path(), 10100, None).await?;
    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    // Print objects owned by `address`
    WalletCommands::Objects { address }
        .execute(&mut context)
        .await?
        .print(true);

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    // Check log output contains all object ids.
    for (object_id, _) in state.object_refs() {
        assert!(logs_contain(format!("{}", object_id).as_str()))
    }

    network.abort();
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_custom_genesis() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    // Create and save genesis config file
    // Create 4 authorities, 1 account with 1 gas object with custom id
    let genesis_path = working_dir.path().join("genesis.conf");
    let mut config = GenesisConfig::default_genesis(&genesis_path)?;
    config.accounts.clear();
    let object_id = ObjectID::random();
    config.accounts.push(AccountConfig {
        address: None,
        gas_objects: vec![ObjectConfig {
            object_id,
            gas_value: 500,
        }],
    });

    let network = start_network(working_dir.path(), 10200, Some(config)).await?;

    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Wallet config
    let wallet_conf = WalletConfig::read(&working_dir.path().join("wallet.conf"))?;
    assert_eq!(1, wallet_conf.accounts.len());

    let address = wallet_conf.accounts.first().unwrap().address;
    let mut context = WalletContext::new(wallet_conf)?;
    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    // Print objects owned by `address`
    WalletCommands::Objects { address }
        .execute(&mut context)
        .await?
        .print(true);

    // confirm the object with custom object id.
    retry_assert!(
        logs_contain(format!("{}", object_id).as_str()),
        Duration::from_millis(5000)
    );

    network.abort();
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_custom_genesis_with_custom_move_package() -> Result<(), anyhow::Error> {
    use sui_types::crypto::get_key_pair_from_bytes;

    let (address, admin_key) = get_key_pair_from_bytes(&[
        10, 112, 5, 142, 174, 127, 187, 146, 251, 68, 22, 191, 128, 68, 84, 13, 102, 71, 77, 57,
        92, 154, 128, 240, 158, 45, 13, 123, 57, 21, 194, 214, 189, 215, 127, 86, 129, 189, 1, 4,
        90, 106, 17, 10, 123, 200, 40, 18, 34, 173, 240, 91, 213, 72, 183, 249, 213, 210, 39, 181,
        105, 254, 59, 163,
    ]);

    let working_dir = tempfile::tempdir()?;
    // Create and save genesis config file
    // Create 4 authorities and 1 account
    let genesis_path = working_dir.path().join("genesis.conf");
    let num_authorities = 4;
    let mut config = GenesisConfig::custom_genesis(&genesis_path, num_authorities, 0, 0)?;
    config.accounts.clear();
    config.accounts.push(AccountConfig {
        address: Some(address),
        gas_objects: vec![],
    });
    config
        .move_packages
        .push(PathBuf::from(TEST_DATA_DIR).join("custom_genesis_package_1"));
    config
        .move_packages
        .push(PathBuf::from(TEST_DATA_DIR).join("custom_genesis_package_2"));
    config.save()?;

    // Create empty network config for genesis
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Genesis
    SuiCommand::Genesis {
        config: Some(genesis_path),
    }
    .execute(&mut config)
    .await?;

    assert!(logs_contain("Loading 2 Move packages"));
    // Checks network config contains package ids
    let network_conf = NetworkConfig::read(&working_dir.path().join("network.conf"))?;
    assert_eq!(2, network_conf.loaded_move_packages.len());

    // Make sure we log out package id
    for (_, id) in network_conf.loaded_move_packages {
        assert!(logs_contain(&*format!("{}", id)));
    }

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });

    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let mut wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    wallet_conf.accounts = vec![AccountInfo {
        address,
        key_pair: admin_key,
    }];
    let mut context = WalletContext::new(wallet_conf)?;

    // Make sure init() is executed correctly for custom_genesis_package_2::M1
    let move_objects = get_move_objects(&mut context, address).await?;
    assert_eq!(move_objects.len(), 1);
    assert_eq!(move_objects[0].type_.module.as_str(), "M1");

    network.abort();
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let network = start_network(working_dir.path(), 10300, None).await?;
    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    // Check log output contains all object ids.
    let object_id = state.object_refs().next().unwrap().0;

    WalletCommands::Object { id: object_id }
        .execute(&mut context)
        .await?
        .print(true);
    let obj_owner = format!("{:?}", address);

    retry_assert!(
        logs_contain(obj_owner.as_str()),
        Duration::from_millis(5000)
    );

    network.abort();
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let network = start_network(working_dir.path(), 10400, None).await?;

    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let recipient = wallet_conf.accounts.get(1).unwrap().address;

    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?;

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    let object_id = state.object_refs().next().unwrap().0;
    let object_to_send = state.object_refs().nth(1).unwrap().0;

    WalletCommands::Gas { address }
        .execute(&mut context)
        .await?
        .print(true);
    let object_id_str = format!("{}", object_id);

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
        object_id: object_to_send,
        gas: object_id,
    }
    .execute(&mut context)
    .await?;

    // Fetch gas again
    WalletCommands::Gas { address }
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

    network.abort();
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
        ObjectID::from_hex(id_str).unwrap(),
        SequenceNumber::from_u64(tokens[1].parse::<u64>().unwrap()),
        tokens[2].parse::<u64>().unwrap(),
    ))
}

async fn get_move_objects(
    context: &mut WalletContext,
    address: SuiAddress,
) -> Result<Vec<MoveObject>, anyhow::Error> {
    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(context)
        .await?
        .print(true);

    // Fetch objects owned by `address`
    let objects_result = WalletCommands::Objects { address }.execute(context).await?;

    match objects_result {
        WalletCommandResult::Objects(object_refs) => {
            let mut objs = vec![];
            for (id, ..) in object_refs {
                objs.push(get_move_object(context, id).await?);
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
) -> Result<MoveObject, anyhow::Error> {
    let obj = WalletCommands::Object { id }.execute(context).await?;

    match obj {
        WalletCommandResult::Object(ObjectRead::Exists(
            _,
            Object {
                data: Data::Move(m),
                ..
            },
            ..,
        )) => Ok(m),
        _ => panic!("WalletCommands::Object returns wrong type {}", obj),
    }
}

async fn start_network(
    working_dir: &Path,
    starting_port: u16,
    genesis: Option<GenesisConfig>,
) -> Result<JoinHandle<Result<(), anyhow::Error>>, anyhow::Error> {
    let network_conf_path = &working_dir.join("network.conf");
    let genesis_conf_path = &working_dir.join("genesis.conf");

    let mut config = NetworkConfig::read_or_create(network_conf_path)?;
    let mut port_allocator = PortAllocator::new(starting_port);
    let mut genesis_config = genesis.unwrap_or(GenesisConfig::default_genesis(genesis_conf_path)?);
    let authorities = genesis_config
        .authorities
        .iter()
        .map(|info| AuthorityPrivateInfo {
            key_pair: info.key_pair.copy(),
            host: info.host.clone(),
            port: port_allocator.next_port().unwrap(),
            db_path: info.db_path.clone(),
            stake: info.stake,
        })
        .collect();
    genesis_config.authorities = authorities;
    genesis_config.save()?;

    SuiCommand::Genesis {
        config: Some(genesis_conf_path.to_path_buf()),
    }
    .execute(&mut config)
    .await?;

    let mut config = NetworkConfig::read(network_conf_path)?;

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });
    Ok(network)
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let network = start_network(working_dir.path(), 10500, None).await?;

    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address1 = wallet_conf.accounts.first().unwrap().address;
    let address2 = wallet_conf.accounts.get(1).unwrap().address;

    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address: address1 }
        .execute(&mut context)
        .await?
        .print(true);
    WalletCommands::SyncClientState { address: address2 }
        .execute(&mut context)
        .await?
        .print(true);

    // Print objects owned by `address1`
    WalletCommands::Objects { address: address1 }
        .execute(&mut context)
        .await?
        .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let state1 = context
        .address_manager
        .get_managed_address_states()
        .get(&address1)
        .unwrap();

    let object_refs = state1
        .object_refs()
        .collect::<BTreeSet<(ObjectID, ObjectRef)>>()
        .clone();
    // Check log output contains all object ids.
    for (object_id, _) in object_refs.clone() {
        assert!(logs_contain(format!("{}", object_id).as_str()))
    }

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let gas = *state1.get_owned_objects().first().unwrap();
    let obj = *state1.get_owned_objects().get(1).unwrap();

    // Create the args
    let addr1_str = format!("0x{:02x}", address1);
    let args_json = json!([123u8, addr1_str]);

    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    let resp = WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: Identifier::new("ObjectBasics").unwrap(),
        function: Identifier::new("create").unwrap(),
        type_args: vec![],
        args,
        gas,
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
        TransactionEffects {
            created: new_objs, ..
        },
    ) = resp
    {
        let ((obj_id, _seq_num, _obj_digest), _owner) = new_objs.first().unwrap();
        *obj_id
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
        module: Identifier::new("ObjectBasics").unwrap(),
        function: Identifier::new("create").unwrap(),
        type_args: vec![],
        args: args.to_vec(),
        gas,
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
        module: Identifier::new("ObjectBasics").unwrap(),
        function: Identifier::new("transfer").unwrap(),
        type_args: vec![],
        args: args.to_vec(),
        gas,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await;

    assert!(resp.is_err());

    let err_string = format!("{} ", resp.err().unwrap());
    assert!(err_string.contains("Expected argument of type 0x2::ObjectBasics::Object, but found type 0x2::Coin::Coin<0x2::GAS::GAS>"));

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
        module: Identifier::new("ObjectBasics").unwrap(),
        function: Identifier::new("transfer").unwrap(),
        type_args: vec![],
        args: args.to_vec(),
        gas,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    retry_assert!(
        logs_contain("Mutated Objects:"),
        Duration::from_millis(1000)
    );
    assert!(logs_contain("Created Objects:"));

    network.abort();
    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let network = start_network(working_dir.path(), 10600, None).await?;
    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    // Check log output contains all object ids.
    let gas_obj_id = state.object_refs().next().unwrap().0;

    // Provide path to well formed package sources
    let mut path = TEST_DATA_DIR.to_owned();
    path.push_str("dummy_modules_publish");

    let resp = WalletCommands::Publish {
        path,
        gas: gas_obj_id,
        gas_budget: 1000,
    }
    .execute(&mut context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let dumy_obj = Object::with_id_owner_for_testing(ObjectID::random(), address);
    // Get the created objects
    let (mut created_obj1, mut created_obj2) = (
        dumy_obj.to_object_reference(),
        dumy_obj.to_object_reference(),
    );

    if let WalletCommandResult::Publish(
        _,
        TransactionEffects {
            created: new_objs, ..
        },
    ) = resp
    {
        (created_obj1, created_obj2) = (new_objs.get(0).unwrap().0, new_objs.get(1).unwrap().0);
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false);
    };

    // One is the actual module, while the other is the object created at init
    retry_assert!(
        logs_contain(&format!("{:02X}", created_obj1.0)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{:02X}", created_obj2.0)),
        Duration::from_millis(5000)
    );

    // Check the objects
    // Init with some value to satisfy the type checker
    let mut cr_obj1 = dumy_obj.clone();

    let resp = WalletCommands::Object { id: created_obj1.0 }
        .execute(&mut context)
        .await?;
    if let WalletCommandResult::Object(ObjectRead::Exists(_, object, _)) = resp {
        cr_obj1 = object;
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false)
    };

    let mut cr_obj2 = dumy_obj;

    let resp = WalletCommands::Object { id: created_obj2.0 }
        .execute(&mut context)
        .await?;
    if let WalletCommandResult::Object(ObjectRead::Exists(_, object, _)) = resp {
        cr_obj2 = object;
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false)
    };

    let (pkg, obj) = if cr_obj1.is_package() {
        (cr_obj1, cr_obj2)
    } else {
        (cr_obj2, cr_obj1)
    };

    assert!(pkg.is_package());
    assert!(!obj.is_package());

    network.abort();
    Ok(())
}

#[allow(clippy::assertions_on_constants)]
#[traced_test]
#[tokio::test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let network = start_network(working_dir.path(), 10700, None).await?;
    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let recipient = wallet_conf.accounts.get(1).unwrap().address;

    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    // Check log output contains all object ids.
    let gas_obj_id = state.object_refs().next().unwrap().0;
    let obj_id = state.object_refs().nth(1).unwrap().0;

    let resp = WalletCommands::Transfer {
        gas: gas_obj_id,
        to: recipient,
        object_id: obj_id,
    }
    .execute(&mut context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let dumy_obj = Object::with_id_owner_for_testing(ObjectID::random(), address);

    // Get the mutated objects
    let (mut_obj1, mut_obj2) =
        if let WalletCommandResult::Transfer(_, _, TransactionEffects { mutated, .. }) = resp {
            (mutated.get(0).unwrap().0, mutated.get(1).unwrap().0)
        } else {
            assert!(false);
            (
                dumy_obj.to_object_reference(),
                dumy_obj.to_object_reference(),
            )
        };

    retry_assert!(
        logs_contain(&format!("{:02X}", mut_obj1.0)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{:02X}", mut_obj2.0)),
        Duration::from_millis(5000)
    );

    // Sync both to fetch objects
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);
    WalletCommands::SyncClientState { address: recipient }
        .execute(&mut context)
        .await?
        .print(true);

    // Check the objects
    let resp = WalletCommands::Object { id: mut_obj1.0 }
        .execute(&mut context)
        .await?;
    let mut_obj1 = if let WalletCommandResult::Object(ObjectRead::Exists(_, object, _)) = resp {
        object
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false);
        dumy_obj.clone()
    };

    let resp = WalletCommands::Object { id: mut_obj2.0 }
        .execute(&mut context)
        .await?;
    let mut_obj2 = if let WalletCommandResult::Object(ObjectRead::Exists(_, object, _)) = resp {
        object
    } else {
        // Fail this way because Panic! causes test issues
        assert!(false);
        dumy_obj
    };

    let (gas, obj) = if mut_obj1.get_single_owner().unwrap() == address {
        (mut_obj1, mut_obj2)
    } else {
        (mut_obj2, mut_obj1)
    };

    assert_eq!(gas.get_single_owner().unwrap(), address);
    assert_eq!(obj.get_single_owner().unwrap(), recipient);

    network.abort();
    Ok(())
}
