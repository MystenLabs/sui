// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    config::{GatewayConfig, GatewayType, WalletConfig},
    keystore::{Keystore, KeystoreType, SuiKeystore},
};
use anyhow::{anyhow, bail};
use base64ct::{Base64, Encoding};
use clap::*;
use narwhal_config;
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::{builder::ConfigBuilder, NetworkConfig};
use sui_config::{genesis_config::GenesisConfig, SUI_GENESIS_FILENAME};
use sui_config::{
    sui_config_dir, Config, PersistedConfig, SUI_FULLNODE_CONFIG, SUI_GATEWAY_CONFIG,
    SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG,
};
use sui_swarm::memory::Swarm;
use sui_types::base_types::decode_bytes_hex;
use sui_types::base_types::SuiAddress;
use tracing::info;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SuiCommand {
    /// Start sui network.
    #[clap(name = "start")]
    Start {
        #[clap(long)]
        config: Option<PathBuf>,
    },
    #[clap(name = "network")]
    Network {
        #[clap(long)]
        config: Option<PathBuf>,
        #[clap(short, long, help = "Dump the public keys of all authorities")]
        dump_addresses: bool,
    },
    #[clap(name = "genesis")]
    Genesis {
        #[clap(long, help = "Start genesis with a given config file")]
        from_config: Option<PathBuf>,
        #[clap(
            long,
            help = "Build a genesis config, write it to the specified path, and exit"
        )]
        write_config: Option<PathBuf>,
        #[clap(long)]
        working_dir: Option<PathBuf>,
        #[clap(short, long, help = "Forces overwriting existing configuration")]
        force: bool,
    },
    #[clap(name = "signtool")]
    SignTool {
        #[clap(long)]
        keystore_path: Option<PathBuf>,
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
        #[clap(long)]
        data: String,
    },
}

impl SuiCommand {
    pub async fn execute(&self) -> Result<(), anyhow::Error> {
        match self {
            SuiCommand::Start { config } => {
                // Load the config of the Sui authority.
                let network_config_path = config
                    .clone()
                    .unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let network_config: NetworkConfig = PersistedConfig::read(&network_config_path)
                    .map_err(|err| {
                        err.context(format!(
                            "Cannot open Sui network config file at {:?}",
                            network_config_path
                        ))
                    })?;

                let mut swarm = Swarm::builder()
                    .narwhal_config(narwhal_config::Parameters {
                        max_header_delay: Duration::from_secs(5),
                        ..Default::default()
                    })
                    .from_network_config(sui_config_dir()?, network_config);
                swarm.launch().await?;

                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    for node in swarm.validators_mut() {
                        node.health_check().await?;
                    }

                    interval.tick().await;
                }
            }
            SuiCommand::Network {
                config,
                dump_addresses,
            } => {
                let config_path = config
                    .clone()
                    .unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let config: NetworkConfig = PersistedConfig::read(&config_path).map_err(|err| {
                    err.context(format!(
                        "Cannot open Sui network config file at {:?}",
                        config_path
                    ))
                })?;

                if *dump_addresses {
                    for validator in config.validator_configs() {
                        println!(
                            "{} - {}",
                            validator.network_address(),
                            validator.sui_address()
                        );
                    }
                }
                Ok(())
            }
            SuiCommand::Genesis {
                working_dir,
                force,
                from_config,
                write_config,
            } => {
                let sui_config_dir = &match working_dir {
                    // if a directory is specified, it must exist (it
                    // will not be created)
                    Some(v) => v.clone(),
                    // create default Sui config dir if not specified
                    // on the command line and if it does not exist
                    // yet
                    None => {
                        let config_path = sui_config_dir()?;
                        fs::create_dir_all(&config_path)?;
                        config_path
                    }
                };

                // if Sui config dir is not empty then either clean it
                // up (if --force/-f option was specified or report an
                // error
                if write_config.is_none()
                    && sui_config_dir
                        .read_dir()
                        .map_err(|err| {
                            anyhow!(err)
                                .context(format!("Cannot open Sui config dir {:?}", sui_config_dir))
                        })?
                        .next()
                        .is_some()
                {
                    if *force {
                        fs::remove_dir_all(sui_config_dir).map_err(|err| {
                            anyhow!(err).context(format!(
                                "Cannot remove Sui config dir {:?}",
                                sui_config_dir
                            ))
                        })?;
                        fs::create_dir(sui_config_dir).map_err(|err| {
                            anyhow!(err).context(format!(
                                "Cannot create Sui config dir {:?}",
                                sui_config_dir
                            ))
                        })?;
                    } else {
                        bail!("Cannot run genesis with non-empty Sui config directory {}, please use --force/-f option to remove existing configuration", sui_config_dir.to_str().unwrap());
                    }
                }

                let network_path = sui_config_dir.join(SUI_NETWORK_CONFIG);
                let genesis_path = sui_config_dir.join(SUI_GENESIS_FILENAME);
                let wallet_path = sui_config_dir.join(SUI_WALLET_CONFIG);
                let gateway_path = sui_config_dir.join(SUI_GATEWAY_CONFIG);
                let keystore_path = sui_config_dir.join("wallet.key");
                let db_folder_path = sui_config_dir.join("client_db");
                let gateway_db_folder_path = sui_config_dir.join("gateway_client_db");

                let mut genesis_conf = match from_config {
                    Some(q) => PersistedConfig::read(q)?,
                    None => GenesisConfig::for_local_testing(),
                };

                if let Some(path) = write_config {
                    let persisted = genesis_conf.persisted(path);
                    persisted.save()?;
                    return Ok(());
                }

                let validator_info = genesis_conf.validator_genesis_info.take();
                let mut network_config = if let Some(validators) = validator_info {
                    ConfigBuilder::new(sui_config_dir)
                        .initial_accounts_config(genesis_conf)
                        .build_with_validators(validators)
                } else {
                    ConfigBuilder::new(sui_config_dir)
                        .committee_size(NonZeroUsize::new(genesis_conf.committee_size).unwrap())
                        .initial_accounts_config(genesis_conf)
                        .build()
                };

                let mut accounts = Vec::new();
                let mut keystore = SuiKeystore::default();

                for key in &network_config.account_keys {
                    let address = SuiAddress::from(key.public_key_bytes());
                    accounts.push(address);
                    keystore.add_key(address, key.copy())?;
                }

                network_config.genesis.save(&genesis_path)?;
                for validator in &mut network_config.validator_configs {
                    validator.genesis = sui_config::node::Genesis::new_from_file(&genesis_path);
                }

                info!("Network genesis completed.");
                network_config.save(&network_path)?;
                info!("Network config file is stored in {:?}.", network_path);

                keystore.set_path(&keystore_path);
                keystore.save()?;
                info!("Wallet keystore is stored in {:?}.", keystore_path);

                // Use the first address if any
                let active_address = accounts.get(0).copied();

                let validator_set = network_config.validator_set();

                GatewayConfig {
                    db_folder_path: gateway_db_folder_path,
                    validator_set: validator_set.to_owned(),
                    ..Default::default()
                }
                .save(&gateway_path)?;
                info!("Gateway config file is stored in {:?}.", gateway_path);

                let wallet_gateway_config = GatewayConfig {
                    db_folder_path,
                    validator_set: validator_set.to_owned(),
                    ..Default::default()
                };

                let wallet_config = WalletConfig {
                    accounts,
                    keystore: KeystoreType::File(keystore_path),
                    gateway: GatewayType::Embedded(wallet_gateway_config),
                    active_address,
                };

                wallet_config.save(&wallet_path)?;
                info!("Wallet config file is stored in {:?}.", wallet_path);

                let mut fullnode_config = network_config.generate_fullnode_config();
                fullnode_config.json_rpc_address = sui_config::node::default_json_rpc_address();
                fullnode_config.websocket_address = sui_config::node::default_websocket_address();
                fullnode_config.save(sui_config_dir.join(SUI_FULLNODE_CONFIG))?;

                for (i, validator) in network_config
                    .into_validator_configs()
                    .into_iter()
                    .enumerate()
                {
                    let path = sui_config_dir.join(format!("validator-config-{}.yaml", i));
                    validator.save(path)?;
                }

                Ok(())
            }
            SuiCommand::SignTool {
                keystore_path,
                address,
                data,
            } => {
                let keystore_path = keystore_path
                    .clone()
                    .unwrap_or(sui_config_dir()?.join("wallet.key"));
                let keystore = SuiKeystore::load_or_create(&keystore_path)?;
                info!("Data to sign : {}", data);
                info!("Address : {}", address);
                let message = Base64::decode_vec(data).map_err(|e| anyhow!(e))?;
                let signature = keystore.sign(address, &message)?;
                // Separate pub key and signature string, signature and pub key are concatenated with an '@' symbol.
                let signature_string = format!("{:?}", signature);
                let sig_split = signature_string.split('@').collect::<Vec<_>>();
                let signature = sig_split
                    .first()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                let pub_key = sig_split
                    .last()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                info!("Public Key Base64: {}", pub_key);
                info!("Signature : {}", signature);
                Ok(())
            }
        }
    }
}
