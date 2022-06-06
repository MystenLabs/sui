// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use sui::{
    config::{GatewayConfig, GatewayType, WalletConfig},
    keystore::{KeystoreType, SuiKeystore},
    wallet_commands::{WalletCommands, WalletContext},
};
use sui_config::genesis_config::GenesisConfig;
use sui_config::{Config, SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG};
use sui_swarm::memory::Swarm;
use sui_types::base_types::SuiAddress;

const NUM_VALIDAOTR: usize = 4;

pub async fn start_test_network(
    genesis_config: Option<GenesisConfig>,
) -> Result<Swarm, anyhow::Error> {
    let mut builder = Swarm::builder().committee_size(NonZeroUsize::new(NUM_VALIDAOTR).unwrap());
    if let Some(genesis_config) = genesis_config {
        builder = builder.initial_accounts_config(genesis_config);
    }

    let mut swarm = builder.build();
    swarm.launch().await?;

    let accounts = swarm
        .config()
        .account_keys
        .iter()
        .map(|key| SuiAddress::from(key.public_key_bytes()))
        .collect::<Vec<_>>();

    let dir = swarm.dir();

    let network_path = dir.join(SUI_NETWORK_CONFIG);
    let wallet_path = dir.join(SUI_WALLET_CONFIG);
    let keystore_path = dir.join("wallet.key");
    let db_folder_path = dir.join("client_db");
    let gateway_path = dir.join(SUI_GATEWAY_CONFIG);

    swarm.config().save(&network_path)?;
    let mut keystore = SuiKeystore::default();
    for key in &swarm.config().account_keys {
        keystore.add_key(SuiAddress::from(key.public_key_bytes()), key.copy())?;
    }
    keystore.set_path(&keystore_path);
    keystore.save()?;

    let validators = swarm.config().validator_set().to_owned();
    let active_address = accounts.get(0).copied();

    GatewayConfig {
        db_folder_path: db_folder_path.clone(),
        validator_set: validators.clone(),
        ..Default::default()
    }
    .save(gateway_path)?;

    // Create wallet config with stated authorities port
    WalletConfig {
        accounts,
        keystore: KeystoreType::File(keystore_path),
        gateway: GatewayType::Embedded(GatewayConfig {
            db_folder_path,
            validator_set: validators,
            ..Default::default()
        }),
        active_address,
    }
    .save(&wallet_path)?;

    // Return network handle
    Ok(swarm)
}

pub async fn setup_network_and_wallet() -> Result<(Swarm, WalletContext, SuiAddress), anyhow::Error>
{
    let swarm = start_test_network(None).await?;

    // Create Wallet context.
    let wallet_conf = swarm.dir().join(SUI_WALLET_CONFIG);
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?;
    Ok((swarm, context, address))
}
