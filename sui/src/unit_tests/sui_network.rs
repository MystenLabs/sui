// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use sui::config::{AuthorityPrivateInfo, Config, GenesisConfig, WalletConfig};
use sui::gateway_config::{GatewayConfig, GatewayType};
use sui::keystore::KeystoreType;
use sui::sui_commands::{genesis, SuiNetwork};
use sui::{SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG};
use sui_types::crypto::{get_key_pair, random_key_pairs};
use sui_types::base_types::SuiAddress;
use tracing::info;

const NUM_VALIDAOTR: usize = 4;

#[cfg(test)]
pub async fn start_test_network(
    working_dir: &Path,
    genesis_config: Option<GenesisConfig>,
    // fixme
) -> Result<SuiNetwork, anyhow::Error> {
    let working_dir = working_dir.to_path_buf();
    let network_path = working_dir.join(SUI_NETWORK_CONFIG);
    let wallet_path = working_dir.join(SUI_WALLET_CONFIG);
    let keystore_path = working_dir.join("wallet.key");
    let db_folder_path = working_dir.join("client_db");

    let key_pairs = random_key_pairs(NUM_VALIDAOTR);

    // let mut genesis_config =
    //     genesis_config.unwrap_or(GenesisConfig::default_genesis(&working_dir)?);

    let mut genesis_config = match genesis_config {
        Some(genesis_config) => genesis_config,
        None => {
            let key_pairs = random_key_pairs(NUM_VALIDAOTR);
            GenesisConfig::default_genesis(&working_dir, Some(key_pairs))?
        }
    };

    genesis_config.authorities = genesis_config
        .authorities
        .into_iter()
        .map(|info| AuthorityPrivateInfo { port: 0, ..info })
        .collect();

    print!("@@@@@@@@ Genesis config. key_pair: {:?}\n", genesis_config.key_pair.public_key_bytes());
    for a in &genesis_config.authorities {
        print!("@@@@@@@ Genesis config. au pub key {:?}\n", a.public_key);
    }
    
    // let (_, key_pair) = get_key_pair();
    // genesis_config.authorities[0].public_key = *key_pair.public_key_bytes();
    // genesis_config.authorities[0].address = SuiAddress::from(key_pair.public_key_bytes());
    // genesis_config.key_pair = key_pair;

    let (network_config, accounts, mut keystore) = genesis(genesis_config).await?;
    let key_pair_refs = key_pairs.iter().map(|kp| kp).collect::<Vec<_>>();
    let network = SuiNetwork::start(&network_config, key_pair_refs).await?;

    let network_config = network_config.persisted(&network_path);
    network_config.save()?;
    print!("@@@@@@@@ Network config. key_pair: {:?}\n", network_config.key_pair.public_key_bytes());
    keystore.set_path(&keystore_path);
    keystore.save()?;

    let authorities = network_config.get_authority_infos();
    let authorities = authorities
        .into_iter()
        .zip(&network.spawned_authorities)
        .map(|(mut info, server)| {
            info.base_port = server.get_port();
            info
        })
        .collect::<Vec<_>>();
    let active_address = accounts.get(0).copied();

    GatewayConfig {
        db_folder_path: db_folder_path.clone(),
        authorities: authorities.clone(),
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
            authorities,
            ..Default::default()
        }),
        active_address,
    }
    .persisted(&wallet_path)
    .save()?;

    // Return network handle
    Ok(network)
}
