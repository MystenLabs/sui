// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::fs::read_dir;
use std::ops::Add;
use std::time::Duration;
use sui::config::{
    AccountConfig, AccountInfo, GenesisConfig, NetworkConfig, ObjectConfig, WalletConfig,
    AUTHORITIES_DB_NAME,
};
use sui::wallet_commands::{WalletCommands, WalletContext};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::crypto::get_key_pair;
use sui_types::object::GAS_VALUE_FOR_TESTING;
use tokio::task;
use tracing_test::traced_test;

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

    // Genesis 2nd time should fail
    let result = SuiCommand::Genesis { config: None }
        .execute(&mut config)
        .await;
    assert!(matches!(result, Err(..)));

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

// TODO: Re-enable after we fix this test.
#[ignore]
#[traced_test]
#[tokio::test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    SuiCommand::Genesis { config: None }
        .execute(&mut config)
        .await?;

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });

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
    config.save()?;

    // Create empty network config for genesis
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Genesis
    SuiCommand::Genesis {
        config: Some(genesis_path),
    }
    .execute(&mut config)
    .await?;

    let mut config = NetworkConfig::read(&working_dir.path().join("network.conf"))?;
    assert_eq!(4, config.authorities.len());

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });

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
    config.move_packages.push(
        PathBuf::from("..")
            .join("sui_programmability")
            .join("examples"),
    );
    config.save()?;

    // Create empty network config for genesis
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Genesis
    SuiCommand::Genesis {
        config: Some(genesis_path),
    }
    .execute(&mut config)
    .await?;

    assert!(logs_contain("Loading 1 Move packages"));
    // Checks network config contains package ids
    let network_conf = NetworkConfig::read(&working_dir.path().join("network.conf"))?;
    assert_eq!(1, network_conf.loaded_move_packages.len());

    // Make sure we log out package id
    for (_, id) in network_conf.loaded_move_packages {
        assert!(logs_contain(&*format!("{}", id)));
    }
    Ok(())
}

// TODO: Re-enable after we fix this test.
#[ignore]
#[traced_test]
#[tokio::test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    SuiCommand::Genesis { config: None }
        .execute(&mut config)
        .await?;

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });

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

// TODO: Re-enable after we fix this test.
#[ignore]
#[traced_test]
#[tokio::test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    SuiCommand::Genesis { config: None }
        .execute(&mut config)
        .await?;

    // Start network
    let network = task::spawn(async move { SuiCommand::Start.execute(&mut config).await });

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
