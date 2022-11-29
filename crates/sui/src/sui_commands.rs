// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::{stderr, stdout, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{anyhow, bail};
use clap::*;
use colored::Colorize;
use fastcrypto::traits::KeyPair;
use move_package::BuildConfig;
use tracing::{info, warn};

use sui_config::{builder::ConfigBuilder, NetworkConfig, SUI_KEYSTORE_FILENAME};
use sui_config::{genesis_config::GenesisConfig, SUI_GENESIS_FILENAME};
use sui_config::{
    sui_config_dir, Config, PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG,
    SUI_NETWORK_CONFIG,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_swarm::memory::Swarm;
use sui_types::crypto::{SignatureScheme, SuiKeyPair};

use crate::client_commands::{SuiClientCommands, WalletContext};
use crate::config::{SuiClientConfig, SuiEnv};
use crate::console::start_console;
use crate::genesis_ceremony::{run, Ceremony};
use crate::keytool::KeyToolCommand;
use crate::sui_move::{self, execute_move_command};

#[allow(clippy::large_enum_variant)]
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
        #[clap(long = "no-full-node")]
        no_full_node: bool,
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
    GenesisCeremony(Ceremony),
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
        /// Path to a package which the command should be run with respect to.
        #[clap(long = "path", short = 'p', global = true, parse(from_os_str))]
        package_path: Option<PathBuf>,
        /// Package build options
        #[clap(flatten)]
        build_config: BuildConfig,
        /// Subcommands.
        #[clap(subcommand)]
        cmd: sui_move::Command,
    },
}

impl SuiCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            SuiCommand::Start {
                config,
                no_full_node,
            } => {
                // Auto genesis if path is none and sui directory doesn't exists.
                if config.is_none() && !sui_config_dir()?.join(SUI_NETWORK_CONFIG).exists() {
                    genesis(None, None, None, false).await?;
                }

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

                let mut swarm = if no_full_node {
                    Swarm::builder()
                } else {
                    Swarm::builder()
                        .with_fullnode_rpc_addr(sui_config::node::default_json_rpc_address())
                }
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
            } => genesis(from_config, write_config, working_dir, force).await,
            SuiCommand::GenesisCeremony(cmd) => run(cmd),
            SuiCommand::KeyTool { keystore_path, cmd } => {
                let keystore_path =
                    keystore_path.unwrap_or(sui_config_dir()?.join(SUI_KEYSTORE_FILENAME));
                let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
                cmd.execute(&mut keystore)
            }
            SuiCommand::Console { config } => {
                let config = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config).await?;
                let context = WalletContext::new(&config, None).await?;
                start_console(context, &mut stdout(), &mut stderr()).await
            }
            SuiCommand::Client { config, cmd, json } => {
                let config_path = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config_path).await?;

                // Server switch need to happen before context creation, or else it might fail due to previously misconfigured url.
                if let Some(SuiClientCommands::Switch { env: Some(env), .. }) = &cmd {
                    let config: SuiClientConfig = PersistedConfig::read(&config_path)?;
                    let mut config = config.persisted(&config_path);
                    SuiClientCommands::switch_env(&mut config, env)?;
                    // This will init the client to check if the urls are correct and reachable
                    config.get_active_env()?.create_rpc_client(None).await?;
                    config.save()?;
                }

                let mut context = WalletContext::new(&config_path, None).await?;

                if let Some(cmd) = cmd {
                    if let Err(e) = context.client.check_api_version() {
                        warn!("{e}");
                        println!("{}", format!("[warn] {e}").yellow().bold());
                    };
                    cmd.execute(&mut context).await?.print(!json);
                } else {
                    // Print help
                    let mut app: Command = SuiCommand::command();
                    app.build();
                    app.find_subcommand_mut("client").unwrap().print_help()?;
                }
                Ok(())
            }
            SuiCommand::Move {
                package_path,
                build_config,
                cmd,
            } => execute_move_command(package_path, build_config, cmd),
        }
    }
}

async fn genesis(
    from_config: Option<PathBuf>,
    write_config: Option<PathBuf>,
    working_dir: Option<PathBuf>,
    force: bool,
) -> Result<(), anyhow::Error> {
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
    let dir = sui_config_dir.read_dir().map_err(|err| {
        anyhow!(err).context(format!("Cannot open Sui config dir {:?}", sui_config_dir))
    })?;
    let files = dir.collect::<Result<Vec<_>, _>>()?;

    let client_path = sui_config_dir.join(SUI_CLIENT_CONFIG);
    let keystore_path = sui_config_dir.join(SUI_KEYSTORE_FILENAME);

    if write_config.is_none() && !files.is_empty() {
        if force {
            // check old keystore and client.yaml is compatible
            let is_compatible = FileBasedKeystore::new(&keystore_path).is_ok()
                && PersistedConfig::<SuiClientConfig>::read(&client_path).is_ok();
            // Keep keystore and client.yaml if they are compatible
            if is_compatible {
                for file in files {
                    let path = file.path();
                    if path != client_path && path != keystore_path {
                        if path.is_file() {
                            fs::remove_file(path)
                        } else {
                            fs::remove_dir_all(path)
                        }
                        .map_err(|err| {
                            anyhow!(err).context(format!("Cannot remove file {:?}", file.path()))
                        })?;
                    }
                }
            } else {
                fs::remove_dir_all(sui_config_dir).map_err(|err| {
                    anyhow!(err)
                        .context(format!("Cannot remove Sui config dir {:?}", sui_config_dir))
                })?;
                fs::create_dir(sui_config_dir).map_err(|err| {
                    anyhow!(err)
                        .context(format!("Cannot create Sui config dir {:?}", sui_config_dir))
                })?;
            }
        } else if files.len() != 2 || !client_path.exists() || !keystore_path.exists() {
            bail!("Cannot run genesis with non-empty Sui config directory {}, please use --force/-f option to remove existing configuration", sui_config_dir.to_str().unwrap());
        }
    }

    let network_path = sui_config_dir.join(SUI_NETWORK_CONFIG);
    let genesis_path = sui_config_dir.join(SUI_GENESIS_FILENAME);

    let mut genesis_conf = match from_config {
        Some(path) => PersistedConfig::read(&path)?,
        None => {
            if keystore_path.exists() {
                let existing_keys = FileBasedKeystore::new(&keystore_path)?.addresses();
                GenesisConfig::for_local_testing_with_addresses(existing_keys)
            } else {
                GenesisConfig::for_local_testing()
            }
        }
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
            .with_validators(validators)
            .build()
    } else {
        ConfigBuilder::new(sui_config_dir)
            .committee_size(NonZeroUsize::new(genesis_conf.committee_size).unwrap())
            .initial_accounts_config(genesis_conf)
            .build()
    };

    let mut keystore = FileBasedKeystore::new(&keystore_path)?;
    for key in &network_config.account_keys {
        keystore.add_key(SuiKeyPair::Ed25519SuiKeyPair(key.copy()))?;
    }
    let active_address = keystore.addresses().pop();

    network_config.genesis.save(&genesis_path)?;
    for validator in &mut network_config.validator_configs {
        validator.genesis = sui_config::node::Genesis::new_from_file(&genesis_path);
    }

    info!("Network genesis completed.");
    network_config.save(&network_path)?;
    info!("Network config file is stored in {:?}.", network_path);

    info!("Client keystore is stored in {:?}.", keystore_path);

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

    let mut client_config = if client_path.exists() {
        PersistedConfig::read(&client_path)?
    } else {
        SuiClientConfig::new(keystore.into())
    };

    if client_config.active_address.is_none() {
        client_config.active_address = active_address;
    }
    client_config.add_env(SuiEnv {
        alias: "localnet".to_string(),
        rpc: format!("http://{}", fullnode_config.json_rpc_address),
        ws: None,
    });
    client_config.add_env(SuiEnv::devnet());

    if client_config.active_env.is_none() {
        client_config.active_env = client_config.envs.first().map(|env| env.alias.clone());
    }

    client_config.save(&client_path)?;
    info!("Client config file is stored in {:?}.", client_path);

    Ok(())
}

async fn prompt_if_no_config(wallet_conf_path: &Path) -> Result<(), anyhow::Error> {
    // Prompt user for connect to devnet fullnode if config does not exist.
    if !wallet_conf_path.exists() {
        let env = match std::env::var_os("SUI_CONFIG_WITH_RPC_URL") {
            Some(v) => Some(SuiEnv {
                alias: "custom".to_string(),
                rpc: v.into_string().unwrap(),
                ws: None,
            }),
            None => {
                print!(
                    "Config file [{:?}] doesn't exist, do you want to connect to a Sui full node server [yN]?",
                    wallet_conf_path
                );
                if matches!(read_line(), Ok(line) if line.trim().to_lowercase() == "y") {
                    print!("Sui full node server url (Default to Sui DevNet if not specified) : ");
                    let url = read_line()?;
                    Some(if url.trim().is_empty() {
                        SuiEnv::devnet()
                    } else {
                        print!("Environment alias for [{url}] : ");
                        let alias = read_line()?;
                        let alias = if alias.trim().is_empty() {
                            "custom".to_string()
                        } else {
                            alias
                        };
                        SuiEnv {
                            alias,
                            rpc: url,
                            ws: None,
                        }
                    })
                } else {
                    None
                }
            }
        };

        if let Some(env) = env {
            let keystore_path = wallet_conf_path
                .parent()
                .unwrap_or(&sui_config_dir()?)
                .join(SUI_KEYSTORE_FILENAME);
            let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
            println!("Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1):");
            let key_scheme = match SignatureScheme::from_flag(read_line()?.trim()) {
                Ok(s) => s,
                Err(e) => return Err(anyhow!("{e}")),
            };
            let (new_address, phrase, scheme) = keystore.generate_new_key(key_scheme, None)?;
            println!(
                "Generated new keypair for address with scheme {:?} [{new_address}]",
                scheme.to_string()
            );
            println!("Secret Recovery Phrase : [{phrase}]");
            let alias = env.alias.clone();
            SuiClientConfig {
                keystore,
                envs: vec![env],
                active_address: Some(new_address),
                active_env: Some(alias),
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
