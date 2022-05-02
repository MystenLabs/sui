// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::path::Path;
use sui::{
    config::{
        AuthorityPrivateInfo, Config, GatewayConfig, GatewayType, GenesisConfig, WalletConfig,
    },
    keystore::KeystoreType,
    sui_commands::{genesis, SuiNetwork},
    SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG,
};
use sui_types::crypto::{random_key_pairs, KeyPair};

const NUM_VALIDAOTR: usize = 4;

#[cfg(test)]
pub async fn start_test_network(
    working_dir: &Path,
    genesis_config: Option<GenesisConfig>,
    key_pairs: Option<Vec<KeyPair>>,
) -> Result<SuiNetwork, anyhow::Error> {
    let working_dir = working_dir.to_path_buf();
    let network_path = working_dir.join(SUI_NETWORK_CONFIG);
    let wallet_path = working_dir.join(SUI_WALLET_CONFIG);
    let keystore_path = working_dir.join("wallet.key");
    let db_folder_path = working_dir.join("client_db");

    if genesis_config.is_none() ^ key_pairs.is_none() {
        return Err(anyhow!(
            "genesis_config and key_pairs should be absent/present in tandem."
        ));
    }

    let key_pairs = key_pairs.unwrap_or_else(|| random_key_pairs(NUM_VALIDAOTR));

    let mut genesis_config = match genesis_config {
        Some(genesis_config) => genesis_config,
        None => GenesisConfig::default_genesis(
            &working_dir,
            Some((
                key_pairs
                    .iter()
                    .map(|kp| *kp.public_key_bytes())
                    .collect::<Vec<_>>(),
                key_pairs[0].copy(),
            )),
        )?,
    };

    if genesis_config.authorities.len() != key_pairs.len() {
        return Err(anyhow!(
            "genesis_config's authority num should match key_pairs's length."
        ));
    }

    genesis_config.authorities = genesis_config
        .authorities
        .into_iter()
        .map(|info| AuthorityPrivateInfo { port: 0, ..info })
        .collect();

    let (network_config, accounts, mut keystore) = genesis(genesis_config).await?;
    let key_pair_refs = key_pairs.iter().collect::<Vec<_>>();
    let network = SuiNetwork::start(&network_config, key_pair_refs).await?;

    let network_config = network_config.persisted(&network_path);
    network_config.save()?;
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
