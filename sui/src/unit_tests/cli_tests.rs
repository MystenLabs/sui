use super::*;
use std::fs::read_dir;
use std::time::Duration;
use sui::config::{AccountInfo, NetworkConfig, WalletConfig};
use sui::wallet_commands::{WalletCommands, WalletContext};
use sui_types::base_types::{encode_address_hex, get_key_pair};
use tokio::task;
use tracing_test::traced_test;

#[traced_test]
#[tokio::test]
async fn test_genesis() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Start network without authorities
    let start = SuiCommand::Start.execute(&mut config).await;
    assert!(matches!(start, Err(..)));
    // Genesis
    SuiCommand::Genesis.execute(&mut config).await?;
    assert!(logs_contain("Network genesis completed."));

    // Get all the new file names
    let files = read_dir(working_dir.path())?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(3, files.len());
    assert!(files.contains(&"wallet.conf".to_string()));
    assert!(files.contains(&"authorities_db".to_string()));
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
    let result = SuiCommand::Genesis.execute(&mut config).await;
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

    WalletCommands::Addresses.execute(&mut context).await?;

    for address in context.config.accounts.iter().map(|info| info.address) {
        assert!(logs_contain(&&*encode_address_hex(&address)));
    }

    Ok(())
}

#[traced_test]
#[tokio::test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    SuiCommand::Genesis.execute(&mut config).await?;

    // Start network
    let network = task::spawn(async move {
        let mut config = config;
        SuiCommand::Start.execute(&mut config).await
    });

    while !logs_contain("Listening to TCP traffic on 127.0.0.1") {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let wallet_conf = WalletConfig::read_or_create(&working_dir.path().join("wallet.conf"))?;
    let address = wallet_conf.accounts.first().unwrap().address;
    let mut context = WalletContext::new(wallet_conf)?;

    // Sync client
    WalletCommands::SyncClientState { address }
        .execute(&mut context)
        .await?;

    WalletCommands::Objects { address }
        .execute(&mut context)
        .await?;

    let state = context
        .address_manager
        .get_managed_address_states()
        .get(&address)
        .unwrap();

    for (object_id, _) in state.object_refs() {
        assert!(logs_contain(format!("{}", object_id).as_str()))
    }

    network.abort();
    Ok(())
}
