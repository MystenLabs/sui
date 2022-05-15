// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{num::NonZeroUsize, path::Path};
use sui::{
    config::{
        Config, GatewayConfig, GatewayType, WalletConfig, SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG,
        SUI_WALLET_CONFIG,
    },
    keystore::{KeystoreType, SuiKeystore},
    sui_commands::SuiNetwork,
};
use sui_config::{builder::ConfigBuilder, GenesisConfig};
use sui_types::base_types::SuiAddress;

const NUM_VALIDAOTR: usize = 4;

pub async fn start_test_network(
    working_dir: &Path,
    genesis_config: Option<GenesisConfig>,
) -> Result<SuiNetwork, anyhow::Error> {
    std::fs::create_dir_all(working_dir)?;
    let working_dir = working_dir.to_path_buf();
    let network_path = working_dir.join(SUI_NETWORK_CONFIG);
    let wallet_path = working_dir.join(SUI_WALLET_CONFIG);
    let keystore_path = working_dir.join("wallet.key");
    let db_folder_path = working_dir.join("client_db");

    let mut builder =
        ConfigBuilder::new(&working_dir).committee_size(NonZeroUsize::new(NUM_VALIDAOTR).unwrap());

    if let Some(genesis_config) = genesis_config {
        builder = builder.initial_accounts_config(genesis_config);
    }

    let network_config = builder.build();
    let accounts = network_config
        .account_keys
        .iter()
        .map(|key| SuiAddress::from(key.public_key_bytes()))
        .collect::<Vec<_>>();
    let network = SuiNetwork::start(&network_config).await?;

    let network_config = network_config.persisted(&network_path);
    network_config.save()?;
    let mut keystore = SuiKeystore::default();
    for key in &network_config.account_keys {
        keystore.add_key(SuiAddress::from(key.public_key_bytes()), key.copy())?;
    }
    keystore.set_path(&keystore_path);
    keystore.save()?;

    let validators = network_config.validator_set().to_owned();
    let active_address = accounts.get(0).copied();

    GatewayConfig {
        db_folder_path: db_folder_path.clone(),
        validator_set: validators.clone(),
        ..Default::default()
    }
    .persisted(&working_dir.join(SUI_GATEWAY_CONFIG))
    .save()?;

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
    .persisted(&wallet_path)
    .save()?;

    // Return network handle
    Ok(network)
}
