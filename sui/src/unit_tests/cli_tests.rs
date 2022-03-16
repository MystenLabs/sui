// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::read_dir;
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::str;
use std::time::Duration;

use move_core_types::identifier::Identifier;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::task;
use tokio::task::JoinHandle;
use tracing_test::traced_test;

use sui::config::{
    AccountConfig, AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig, ObjectConfig,
    PersistedConfig, WalletConfig, AUTHORITIES_DB_NAME,
};
use sui::gateway::{EmbeddedGatewayConfig, GatewayType};
use sui::keystore::{KeystoreType, SuiKeystore};
use sui::sui_json::SuiJsonValue;
use sui::wallet_commands::{WalletCommandResult, WalletCommands, WalletContext};
use sui_network::network::PortAllocator;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::messages::TransactionEffects;
use sui_types::object::{Object, ObjectRead, GAS_VALUE_FOR_TESTING};

use super::*;

const TEST_DATA_DIR: &str = "src/unit_tests/data/";
const AIRDROP_SOURCE_CONTRACT_ADDRESS: &str = "bc4ca0eda7647a8ab7c2061c2e118a18a936f13d";
const AIRDROP_SOURCE_TOKEN_ID: u64 = 101u64;
const AIRDROP_TOKEN_NAME: &str = "BoredApeYachtClub";
const AIRDROP_TOKEN_URI: &str = "ipfs://QmeSjSinHpPnmXmspMjwiXyN6zS4E9zccariGR3jxcaWtq/101";

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
    let config = working_dir.join("network.conf");

    // Start network without authorities
    let start = SuiCommand::Start { config }.execute().await;
    assert!(matches!(start, Err(..)));
    // Genesis
    SuiCommand::Genesis {
        working_dir: working_dir.to_path_buf(),
        config: None,
    }
    .execute()
    .await?;
    assert!(logs_contain("Network genesis completed."));

    // Get all the new file names
    let files = read_dir(working_dir)?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(4, files.len());
    assert!(files.contains(&"wallet.conf".to_string()));
    assert!(files.contains(&AUTHORITIES_DB_NAME.to_string()));
    assert!(files.contains(&"network.conf".to_string()));
    assert!(files.contains(&"wallet.key".to_string()));

    // Check network.conf
    let network_conf = PersistedConfig::<NetworkConfig>::read(&working_dir.join("network.conf"))?;
    assert_eq!(4, network_conf.authorities.len());

    // Check wallet.conf
    let wallet_conf = PersistedConfig::<WalletConfig>::read(&working_dir.join("wallet.conf"))?;

    if let GatewayType::Embedded(config) = &wallet_conf.gateway {
        assert_eq!(4, config.authorities.len());
        assert_eq!(working_dir.join("client_db"), config.db_folder_path);
    } else {
        panic!()
    }

    assert_eq!(5, wallet_conf.accounts.len());

    // Genesis 2nd time should fail
    let result = SuiCommand::Genesis {
        working_dir: working_dir.to_path_buf(),
        config: None,
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
        gateway: GatewayType::Embedded(EmbeddedGatewayConfig {
            db_folder_path: working_dir.join("client_db"),
            ..Default::default()
        }),
    };
    let wallet_conf_path = working_dir.join("wallet.conf");
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
        assert!(logs_contain(&*format!("{}", address)));
    }

    Ok(())
}

#[traced_test]
#[tokio::test]
// TODO<https://github.com/MystenLabs/sui/issues/505> move this function to a standalone file
async fn test_cross_chain_airdrop() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let network = start_network(working_dir.path(), 10800, None).await?;
    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context with the oracle account
    let (oracle_address, mut context) = airdrop_get_wallet_context_with_oracle(working_dir).await?;
    let recipient_address = *context.config.accounts.first().unwrap();

    // Assemble the move call to claim the airdrop
    let oracle_obj_str = format!(
        "0x{:02x}",
        airdrop_get_oracle_object(oracle_address, &mut context).await?
    );
    let args_json = json!([
        oracle_obj_str,
        format!("0x{:02x}", recipient_address),
        AIRDROP_SOURCE_CONTRACT_ADDRESS.to_string(),
        json!(AIRDROP_SOURCE_TOKEN_ID),
        AIRDROP_TOKEN_NAME.to_string(),
        AIRDROP_TOKEN_URI.to_string()
    ]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    // Claim the airdrop
    let gas_object_id = transfer_gas(recipient_address, oracle_address, &mut context).await?;
    let token = airdrop_call_move_and_get_created_object(args, gas_object_id, &mut context).await?;

    // Verify the airdrop token
    assert_eq!(
        token["contents"]["type"],
        ("0x2::NFT::NFT<0x2::CrossChainAirdrop::ERC721>")
    );
    let nft_data = &token["contents"]["fields"]["data"];
    let erc721_metadata = &nft_data["fields"]["metadata"];
    assert_eq!(
        erc721_metadata["fields"]["token_id"]["fields"]["id"],
        AIRDROP_SOURCE_TOKEN_ID
    );

    // TODO: verify the other string fields once SuiJSON has better support for rendering
    // string fields

    network.abort();
    Ok(())
}

async fn airdrop_get_wallet_context_with_oracle(
    working_dir: TempDir,
) -> Result<(SuiAddress, WalletContext), anyhow::Error> {
    use sui_types::crypto::get_key_pair_from_bytes;

    let (oracle_address, keypair) = get_key_pair_from_bytes(&[
        143, 102, 49, 171, 56, 173, 188, 83, 154, 218, 98, 200, 173, 252, 53, 239, 131, 210, 147,
        14, 4, 24, 132, 151, 178, 0, 167, 89, 176, 90, 106, 176, 208, 47, 8, 58, 177, 56, 246, 192,
        244, 88, 202, 115, 9, 82, 3, 184, 18, 236, 128, 199, 22, 37, 255, 146, 103, 34, 0, 240,
        255, 163, 60, 174,
    ]);
    let wallet_conf_path = working_dir.path().join("wallet.conf");
    let wallet_conf: WalletConfig = PersistedConfig::read(&wallet_conf_path)?;
    let mut wallet_conf = wallet_conf.persisted(&wallet_conf_path);
    let path = match &wallet_conf.keystore {
        KeystoreType::File(path) => path,
        _ => panic!("Unexpected KeystoreType"),
    };
    let mut store = SuiKeystore::load_or_create(path)?;
    store.add_key(oracle_address, keypair)?;
    store.save(path)?;

    wallet_conf.accounts.push(oracle_address);
    wallet_conf.save()?;

    Ok((oracle_address, WalletContext::new(&wallet_conf_path)?))
}

async fn airdrop_get_oracle_object(
    address: SuiAddress,
    context: &mut WalletContext,
) -> Result<ObjectID, anyhow::Error> {
    WalletCommands::SyncClientState { address }
        .execute(context)
        .await?
        .print(true);
    let object_refs = context.gateway.get_owned_objects(address);
    assert_eq!(object_refs.len(), 1);
    Ok(object_refs.first().unwrap().0)
}

async fn airdrop_call_move_and_get_created_object(
    args: Vec<SuiJsonValue>,
    gas: ObjectID,
    context: &mut WalletContext,
) -> Result<Value, anyhow::Error> {
    let resp = WalletCommands::Call {
        package: ObjectID::from_hex_literal("0x2").unwrap(),
        module: Identifier::new("CrossChainAirdrop").unwrap(),
        function: Identifier::new("claim").unwrap(),
        type_args: vec![],
        args: args.to_vec(),
        gas,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;

    let minted_token_id = match resp {
        WalletCommandResult::Call(
            _,
            TransactionEffects {
                created: new_objs, ..
            },
        ) => {
            assert_eq!(new_objs.len(), 1);
            new_objs[0].0 .0
        }
        _ => panic!("unexpected WalletCommandResult"),
    };

    get_move_object(context, minted_token_id).await
}

async fn transfer_gas(
    from: SuiAddress,
    to: SuiAddress,
    context: &mut WalletContext,
) -> Result<ObjectID, anyhow::Error> {
    let gas_objects_result = WalletCommands::Gas { address: from }
        .execute(context)
        .await?;

    let gas_objects = match gas_objects_result {
        WalletCommandResult::Gas(objs) => objs,
        _ => panic!("unexpected WalletCommandResult"),
    };

    WalletCommands::Transfer {
        to,
        object_id: *gas_objects[0].id(),
        gas: *gas_objects[1].id(),
    }
    .execute(context)
    .await?;
    Ok(*gas_objects[0].id())
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
    let mut context = WalletContext::new(&working_dir.path().join("wallet.conf"))?;
    let address = context.config.accounts.first().cloned().unwrap();

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

    let object_refs = context.gateway.get_owned_objects(address);

    // Check log output contains all object ids.
    for (object_id, _, _) in object_refs {
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
    let mut config = GenesisConfig::default_genesis(working_dir.path())?;
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

    let mut context = WalletContext::new(&working_dir.path().join("wallet.conf"))?;
    assert_eq!(1, context.config.accounts.len());
    let address = context.config.accounts.first().cloned().unwrap();

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

    let (address, _) = get_key_pair_from_bytes(&[
        10, 112, 5, 142, 174, 127, 187, 146, 251, 68, 22, 191, 128, 68, 84, 13, 102, 71, 77, 57,
        92, 154, 128, 240, 158, 45, 13, 123, 57, 21, 194, 214, 189, 215, 127, 86, 129, 189, 1, 4,
        90, 106, 17, 10, 123, 200, 40, 18, 34, 173, 240, 91, 213, 72, 183, 249, 213, 210, 39, 181,
        105, 254, 59, 163,
    ]);

    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    // Create and save genesis config file
    // Create 4 authorities and 1 account
    let genesis_path = working_dir.join("genesis.conf");
    let num_authorities = 4;
    let config = GenesisConfig::custom_genesis(working_dir, num_authorities, 0, 0)?;
    let mut config = config.persisted(&genesis_path);

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

    // Genesis
    SuiCommand::Genesis {
        working_dir: working_dir.to_path_buf(),
        config: Some(genesis_path),
    }
    .execute()
    .await?;

    assert!(logs_contain("Loading 2 Move packages"));
    // Checks network config contains package ids
    let network_conf = PersistedConfig::<NetworkConfig>::read(&working_dir.join("network.conf"))?;
    assert_eq!(2, network_conf.loaded_move_packages.len());

    // Make sure we log out package id
    for (_, id) in &network_conf.loaded_move_packages {
        assert!(logs_contain(&*format!("{}", id)));
    }

    // Start network
    let config = working_dir.join("network.conf");
    let network = task::spawn(async move { SuiCommand::Start { config }.execute().await });

    // Wait for authorities to come alive.
    retry_assert!(
        logs_contain("Listening to TCP traffic on 127.0.0.1"),
        Duration::from_millis(5000)
    );

    // Create Wallet context.
    let wallet_conf_path = working_dir.join("wallet.conf");
    let wallet_conf: WalletConfig = PersistedConfig::read(&wallet_conf_path)?;
    let mut wallet_conf = wallet_conf.persisted(&wallet_conf_path);
    wallet_conf.accounts = vec![address];
    wallet_conf.save()?;
    let mut context = WalletContext::new(&wallet_conf_path)?;

    // Make sure init() is executed correctly for custom_genesis_package_2::M1
    let move_objects = get_move_objects(&mut context, address).await?;
    assert_eq!(move_objects.len(), 1);
    assert!(move_objects[0]["contents"]["type"]
        .to_string()
        .contains("M1::Object"));

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
    let wallet_conf = working_dir.path().join("wallet.conf");
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let object_refs = context.gateway.get_owned_objects(address);

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().0;

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
    let wallet_conf = working_dir.path().join("wallet.conf");
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();
    let recipient = context.config.accounts.get(1).cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?;

    let object_refs = context.gateway.get_owned_objects(address);

    let object_id = object_refs.first().unwrap().0;
    let object_to_send = object_refs.get(1).unwrap().0;

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
) -> Result<Vec<Value>, anyhow::Error> {
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
) -> Result<Value, anyhow::Error> {
    let obj = WalletCommands::Object { id }.execute(context).await?;

    match obj {
        WalletCommandResult::Object(obj) => match obj {
            ObjectRead::Exists(_, obj, layout) => {
                Ok(obj.to_json(&layout).unwrap_or_else(|_| json!("")))
            }
            _ => panic!("WalletCommands::Object returns wrong type"),
        },
        _ => panic!("WalletCommands::Object returns wrong type {}", obj),
    }
}

async fn start_network(
    working_dir: &Path,
    starting_port: u16,
    genesis: Option<GenesisConfig>,
) -> Result<JoinHandle<Result<(), anyhow::Error>>, anyhow::Error> {
    let working_dir = working_dir.to_path_buf();
    let network_conf_path = working_dir.join("network.conf");
    let genesis_conf_path = working_dir.join("genesis.conf");

    let mut port_allocator = PortAllocator::new(starting_port);
    let mut genesis_config = genesis.unwrap_or(GenesisConfig::default_genesis(&working_dir)?);
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

    let genesis_config = genesis_config.persisted(&genesis_conf_path);
    genesis_config.save()?;

    SuiCommand::Genesis {
        working_dir,
        config: Some(genesis_conf_path),
    }
    .execute()
    .await?;

    // Start network
    let network = task::spawn(async move {
        SuiCommand::Start {
            config: network_conf_path,
        }
        .execute()
        .await
    });
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
    let wallet_conf = working_dir.path().join("wallet.conf");

    let mut context = WalletContext::new(&wallet_conf)?;
    let address1 = context.config.accounts.first().cloned().unwrap();
    let address2 = context.config.accounts.get(1).cloned().unwrap();

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

    let object_refs = context.gateway.get_owned_objects(address1);

    // Check log output contains all object ids.
    for (object_id, _, _) in &object_refs {
        assert!(logs_contain(format!("{}", object_id).as_str()))
    }

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let gas = object_refs.first().unwrap().0;
    let obj = object_refs.get(1).unwrap().0;

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
    let wallet_conf = working_dir.path().join("wallet.conf");
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let object_refs = context.gateway.get_owned_objects(address);

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().0;

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

    let (package, created_obj) = if let WalletCommandResult::Publish(resppnse) = resp {
        (
            resppnse.package,
            resppnse.created_objects[0].compute_object_reference(),
        )
    } else {
        unreachable!("Invaldi response");
    };

    // One is the actual module, while the other is the object created at init
    retry_assert!(
        logs_contain(&format!("{}", package.0)),
        Duration::from_millis(5000)
    );
    retry_assert!(
        logs_contain(&format!("{}", created_obj.0)),
        Duration::from_millis(5000)
    );

    // Check the objects
    let resp = WalletCommands::Object { id: package.0 }
        .execute(&mut context)
        .await?;
    assert!(matches!(
        resp,
        WalletCommandResult::Object(ObjectRead::Exists(..))
    ));

    let resp = WalletCommands::Object { id: created_obj.0 }
        .execute(&mut context)
        .await?;
    assert!(matches!(
        resp,
        WalletCommandResult::Object(ObjectRead::Exists(..))
    ));

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
    let wallet_conf = working_dir.path().join("wallet.conf");

    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();
    let recipient = context.config.accounts.get(1).cloned().unwrap();
    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?
        .print(true);

    let object_refs = context.gateway.get_owned_objects(address);

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().0;
    let obj_id = object_refs.get(1).unwrap().0;

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
                dumy_obj.compute_object_reference(),
                dumy_obj.compute_object_reference(),
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
