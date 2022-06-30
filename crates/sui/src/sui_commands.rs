// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_commands::{SuiClientCommands, WalletContext};
use crate::config::{GatewayConfig, GatewayType, SuiClientConfig};
use crate::console::start_console;
use crate::keytool::KeyToolCommand;
use crate::sui_move::MoveCommands;
use anyhow::{anyhow, bail};
use clap::*;
use std::io::{stderr, stdout, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::{fs, io};
use sui_config::{builder::ConfigBuilder, NetworkConfig, SUI_DEV_NET_URL, SUI_KEYSTORE_FILENAME};
use sui_config::{genesis_config::GenesisConfig, SUI_GENESIS_FILENAME};
use sui_config::{
    sui_config_dir, Config, PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG,
    SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG,
};
use sui_json_rpc_api::client::SuiRpcClient;
use sui_json_rpc_api::keystore::{KeystoreType, SuiKeystore};
use sui_swarm::memory::Swarm;
use sui_types::base_types::SuiAddress;
use tracing::info;

#[derive(Parser)]
#[clap(
    name = "sui",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum SuiCommand {
    /// Start sui network.
    #[clap(name = "start")]
    Start {
        #[clap(long = "network.config")]
        config: Option<PathBuf>,
    },
    #[clap(name = "network")]
    Network {
        #[clap(long = "network.config")]
        config: Option<PathBuf>,
        #[clap(short, long, help = "Dump the public keys of all authorities")]
        dump_addresses: bool,
    },
    /// Bootstrap and initialize a new sui network
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
    /// Sui keystore tool.
    #[clap(name = "keytool")]
    KeyTool {
        #[clap(long)]
        keystore_path: Option<PathBuf>,
        /// Subcommands.
        #[clap(subcommand)]
        cmd: KeyToolCommand,
    },
    /// Start Sui interactive console.
    #[clap(name = "console")]
    Console {
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
    },
    /// Client for interacting with the Sui network.
    #[clap(name = "client")]
    Client {
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
        #[clap(subcommand)]
        cmd: Option<SuiClientCommands>,
        /// Return command outputs in json format.
        #[clap(long, global = true)]
        json: bool,
    },

    /// Tool to build and test Move applications.
    #[clap(name = "move")]
    Move {
        /// Path to the Move project root.
        #[clap(long, default_value = "./")]
        path: String,
        /// Whether we are building/testing the std/framework code.
        #[clap(long)]
        std: bool,
        /// Subcommands.
        #[clap(subcommand)]
        cmd: MoveCommands,
    },
}

impl SuiCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
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

                let mut swarm =
                    Swarm::builder().from_network_config(sui_config_dir()?, network_config);
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
                let config_path = config.unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let config: NetworkConfig = PersistedConfig::read(&config_path).map_err(|err| {
                    err.context(format!(
                        "Cannot open Sui network config file at {:?}",
                        config_path
                    ))
                })?;

                if dump_addresses {
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
                    Some(v) => v,
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
                    if force {
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
                let client_path = sui_config_dir.join(SUI_CLIENT_CONFIG);
                let gateway_path = sui_config_dir.join(SUI_GATEWAY_CONFIG);
                let keystore_path = sui_config_dir.join(SUI_KEYSTORE_FILENAME);
                let db_folder_path = sui_config_dir.join("client_db");
                let gateway_db_folder_path = sui_config_dir.join("gateway_client_db");

                let mut genesis_conf = match from_config {
                    Some(path) => PersistedConfig::read(&path)?,
                    None => GenesisConfig::for_local_testing(),
                };

                if let Some(path) = write_config {
                    let persisted = genesis_conf.persisted(&path);
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
                info!("Client keystore is stored in {:?}.", keystore_path);

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

                let wallet_config = SuiClientConfig {
                    accounts,
                    keystore: KeystoreType::File(keystore_path),
                    gateway: GatewayType::Embedded(wallet_gateway_config),
                    active_address,
                };

                wallet_config.save(&client_path)?;
                info!("Client config file is stored in {:?}.", client_path);

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
            SuiCommand::KeyTool { keystore_path, cmd } => {
                let keystore_path =
                    keystore_path.unwrap_or(sui_config_dir()?.join(SUI_KEYSTORE_FILENAME));
                let keystore = SuiKeystore::load_or_create(&keystore_path)?;
                cmd.execute(keystore)
            }
            SuiCommand::Console { config } => {
                let config = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config)?;
                let mut context = WalletContext::new(&config)?;
                sync_accounts(&mut context).await?;
                start_console(context, &mut stdout(), &mut stderr()).await
            }
            SuiCommand::Client { config, cmd, json } => {
                let config = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config)?;
                let mut context = WalletContext::new(&config)?;

                if let Some(cmd) = cmd {
                    // Do not sync if command is a gateway switch, as the current gateway might be unreachable and causes sync to panic.
                    if !matches!(
                        cmd,
                        SuiClientCommands::Switch {
                            gateway: Some(_),
                            ..
                        }
                    ) {
                        sync_accounts(&mut context).await?;
                    }
                    cmd.execute(&mut context).await?.print(!json);
                } else {
                    // Print help
                    let mut app: Command = SuiCommand::command();
                    app.build();
                    app.find_subcommand_mut("client").unwrap().print_help()?;
                }
                Ok(())
            }
            SuiCommand::Move { path, std, cmd } => cmd.execute(path.as_ref(), std),
        }
    }
}

// Sync all accounts on start up.
async fn sync_accounts(context: &mut WalletContext) -> Result<(), anyhow::Error> {
    for address in context.config.accounts.clone() {
        SuiClientCommands::SyncClientState {
            address: Some(address),
        }
        .execute(context)
        .await?;
    }
    Ok(())
}

fn prompt_if_no_config(wallet_conf_path: &Path) -> Result<(), anyhow::Error> {
    // Prompt user for connect to gateway if config not exists.
    if !wallet_conf_path.exists() {
        print!(
            "Config file [{:?}] doesn't exist, do you want to connect to a Sui RPC server [yN]?",
            wallet_conf_path
        );
        if matches!(read_line(), Ok(line) if line.trim().to_lowercase() == "y") {
            print!("Sui RPC server Url (Default to Sui DevNet if not specified) : ");
            let url = read_line()?;
            let url = if url.trim().is_empty() {
                SUI_DEV_NET_URL
            } else {
                &url
            };

            // Check url is valid
            SuiRpcClient::new(url)?;
            let keystore_path = wallet_conf_path
                .parent()
                .unwrap_or(&sui_config_dir()?)
                .join(SUI_KEYSTORE_FILENAME);
            let keystore = KeystoreType::File(keystore_path);
            let new_address = keystore.init()?.add_random_key()?;
            SuiClientConfig {
                accounts: vec![new_address],
                keystore,
                gateway: GatewayType::RPC(url.to_string()),
                active_address: Some(new_address),
            }
            .persisted(wallet_conf_path)
            .save()?;
        }
    }
    Ok(())
}

fn read_line() -> Result<String, anyhow::Error> {
    let mut s = String::new();
    let _ = stdout().flush();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim_end().to_string())
}
