use super::*;
use std::fs::read_dir;
use sui::config::NetworkConfig;
use tracing_test::traced_test;

#[traced_test]
#[tokio::test]
async fn test_sui() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let mut config = NetworkConfig::read_or_create(&working_dir.path().join("network.conf"))?;

    // Start network without authorities
    let start = start_network(&config).await;
    assert!(matches!(start, Err(..)));
    // Genesis
    genesis(&mut config).await?;
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
    let result = genesis(&mut config).await;
    assert!(matches!(result, Err(..)));

    working_dir.close()?;
    Ok(())
}
