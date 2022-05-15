// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    config::{
        sui_config_dir, Config, GatewayConfig, GatewayType, PersistedConfig, WalletConfig,
        SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG,
    },
    keystore::{Keystore, KeystoreType, SuiKeystore},
};
use anyhow::{anyhow, bail};
use base64ct::{Base64, Encoding};
use clap::*;
use futures::future::join_all;
use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_adapter::adapter::generate_package_id;
use sui_adapter::genesis;
use sui_config::NetworkConfig;
use sui_config::{GenesisConfig, ValidatorConfig};
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_active::ActiveAuthority;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::authority_server::AuthorityServer;
use sui_core::authority_server::AuthorityServerHandle;
use sui_core::consensus_adapter::ConsensusListener;
use sui_types::base_types::{decode_bytes_hex, ObjectID};
use sui_types::base_types::{SequenceNumber, SuiAddress, TxContext};
use sui_types::error::SuiResult;
use sui_types::object::Object;
use tokio::sync::mpsc::channel;
use tracing::{error, info};

pub const SUI_AUTHORITY_KEYS: &str = "authorities.key";

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

                // Start a sui validator (including its consensus node).
                SuiNetwork::start(&network_config)
                    .await?
                    .wait_for_completion()
                    .await
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
                let wallet_path = sui_config_dir.join(SUI_WALLET_CONFIG);
                let gateway_path = sui_config_dir.join(SUI_GATEWAY_CONFIG);
                let keystore_path = sui_config_dir.join("wallet.key");
                let db_folder_path = sui_config_dir.join("client_db");
                let gateway_db_folder_path = sui_config_dir.join("gateway_client_db");

                let genesis_conf = match from_config {
                    Some(q) => PersistedConfig::read(q)?,
                    None => GenesisConfig::for_local_testing()?,
                };

                if let Some(path) = write_config {
                    let persisted = genesis_conf.persisted(path);
                    persisted.save()?;
                    return Ok(());
                }

                let (network_config, accounts, mut keystore) = genesis(genesis_conf).await?;
                info!("Network genesis completed.");
                let network_config = network_config.persisted(&network_path);
                network_config.save()?;
                info!("Network config file is stored in {:?}.", network_path);
                keystore.set_path(&keystore_path);
                keystore.save()?;
                info!("Wallet keystore is stored in {:?}.", keystore_path);

                // Use the first address if any
                let active_address = accounts.get(0).copied();

                let validator_set = network_config.validator_configs()[0]
                    .committee_config()
                    .validator_set();

                GatewayConfig {
                    db_folder_path: gateway_db_folder_path,
                    validator_set: validator_set.to_owned(),
                    ..Default::default()
                }
                .persisted(&gateway_path)
                .save()?;
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

                let wallet_config = wallet_config.persisted(&wallet_path);
                wallet_config.save()?;
                info!("Wallet config file is stored in {:?}.", wallet_path);

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

pub struct SuiNetwork {
    pub spawned_authorities: Vec<AuthorityServerHandle>,
}

impl SuiNetwork {
    pub async fn start(config: &NetworkConfig) -> Result<Self, anyhow::Error> {
        if config.validator_configs().is_empty() {
            return Err(anyhow!(
                "No authority configured for the network, please run genesis."
            ));
        }

        info!(
            "Starting network with {} authorities",
            config.validator_configs().len()
        );

        let mut spawned_authorities = Vec::new();
        for validator in config.validator_configs() {
            let server = make_server(validator).await?;
            spawned_authorities.push(server.spawn().await?);
        }
        info!("Started {} authorities", spawned_authorities.len());

        Ok(Self {
            spawned_authorities,
        })
    }

    pub async fn kill(self) -> Result<(), anyhow::Error> {
        for spawned_server in self.spawned_authorities {
            spawned_server.kill().await?;
        }
        Ok(())
    }

    pub async fn wait_for_completion(self) -> Result<(), anyhow::Error> {
        let mut handles = Vec::new();
        for spawned_server in self.spawned_authorities {
            handles.push(async move {
                if let Err(err) = spawned_server.join().await {
                    error!("Server ended with an error: {err}");
                }
            });
        }
        join_all(handles).await;
        info!("All servers stopped.");
        Ok(())
    }
}

pub async fn genesis(
    genesis_conf: GenesisConfig,
) -> Result<(NetworkConfig, Vec<SuiAddress>, SuiKeystore), anyhow::Error> {
    let num_to_provision = genesis_conf.committee_size;

    info!("Creating {} new authorities...", num_to_provision);

    let config_dir = sui_config_dir().unwrap();
    let mut network_config = NetworkConfig::generate(&config_dir, num_to_provision);

    let mut addresses = Vec::new();
    let mut preload_modules: Vec<Vec<CompiledModule>> = Vec::new();
    let mut preload_objects = Vec::new();
    let mut all_preload_objects_set = BTreeSet::new();

    info!("Creating accounts and gas objects...",);

    let mut keystore = SuiKeystore::default();
    for account in genesis_conf.accounts {
        let address = if let Some(address) = account.address {
            address
        } else {
            keystore.add_random_key()?
        };

        addresses.push(address);
        let mut preload_objects_map = BTreeMap::new();

        // Populate gas itemized objects
        account.gas_objects.iter().for_each(|q| {
            if !all_preload_objects_set.contains(&q.object_id) {
                preload_objects_map.insert(q.object_id, q.gas_value);
            }
        });

        // Populate ranged gas objects
        if let Some(ranges) = account.gas_object_ranges {
            for rg in ranges {
                let ids = ObjectID::in_range(rg.offset, rg.count)?;

                for obj_id in ids {
                    if !preload_objects_map.contains_key(&obj_id)
                        && !all_preload_objects_set.contains(&obj_id)
                    {
                        preload_objects_map.insert(obj_id, rg.gas_value);
                        all_preload_objects_set.insert(obj_id);
                    }
                }
            }
        }

        for (object_id, value) in preload_objects_map {
            let new_object = Object::with_id_owner_gas_coin_object_for_testing(
                object_id,
                SequenceNumber::new(),
                address,
                value,
            );
            preload_objects.push(new_object);
        }
    }

    info!(
        "Loading Move framework lib from {:?}",
        genesis_conf.move_framework_lib_path
    );
    let move_lib = sui_framework::get_move_stdlib_modules(&genesis_conf.move_framework_lib_path)?;
    preload_modules.push(move_lib);

    // Load Sui and Move framework lib
    info!(
        "Loading Sui framework lib from {:?}",
        genesis_conf.sui_framework_lib_path
    );
    let sui_lib = sui_framework::get_sui_framework_modules(&genesis_conf.sui_framework_lib_path)?;
    preload_modules.push(sui_lib);

    // TODO: allow custom address to be used after the Gateway refactoring
    // Default to use the last address in the wallet config for initializing modules.
    // If there's no address in wallet config, then use 0x0
    let null_address = SuiAddress::default();
    let module_init_address = addresses.last().unwrap_or(&null_address);
    let mut genesis_ctx = genesis::get_genesis_context_with_custom_address(module_init_address);
    // Build custom move packages
    if !genesis_conf.move_packages.is_empty() {
        info!(
            "Loading {} Move packages from {:?}",
            &genesis_conf.move_packages.len(),
            &genesis_conf.move_packages
        );

        for path in genesis_conf.move_packages {
            let mut modules =
                sui_framework::build_move_package(&path, BuildConfig::default(), false)?;

            let package_id = generate_package_id(&mut modules, &mut genesis_ctx)?;

            info!("Loaded package [{}] from {:?}.", package_id, path);
            // Writing package id to network config for user to retrieve later.
            network_config.add_move_package(path, package_id);
            preload_modules.push(modules)
        }
    }

    for validator in network_config.validator_configs() {
        make_server_with_genesis_ctx(
            validator,
            preload_modules.clone(),
            &preload_objects,
            &mut genesis_ctx.clone(),
        )
        .await?;
    }

    Ok((network_config, addresses, keystore))
}

pub async fn make_server(validator_config: &ValidatorConfig) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(validator_config.db_path(), None));
    let name = validator_config.public_key();
    let state = AuthorityState::new_without_genesis(
        validator_config.committee_config().committee(),
        name,
        Arc::pin(validator_config.key_pair().copy()),
        store,
    )
    .await;

    make_authority(validator_config, state).await
}

pub async fn make_server_with_genesis(
    validator_config: &ValidatorConfig,
) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(validator_config.db_path(), None));
    let name = validator_config.public_key();
    let state = AuthorityState::new_with_genesis(
        validator_config.committee_config().committee(),
        name,
        Arc::pin(validator_config.key_pair().copy()),
        store,
        validator_config.genesis(),
    )
    .await;

    make_authority(validator_config, state).await
}

async fn make_server_with_genesis_ctx(
    validator_config: &ValidatorConfig,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    genesis_ctx: &mut TxContext,
) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(validator_config.db_path(), None));
    let name = *validator_config.key_pair().public_key_bytes();

    let state = AuthorityState::new(
        validator_config.committee_config().committee(),
        name,
        Arc::pin(validator_config.key_pair().copy()),
        store,
        preload_modules,
        genesis_ctx,
    )
    .await;

    // Okay to do this since we're at genesis
    state
        .insert_genesis_objects_bulk_unsafe(&preload_objects.iter().collect::<Vec<_>>())
        .await;

    let (tx_sui_to_consensus, _rx_sui_to_consensus) = channel(1);
    Ok(AuthorityServer::new(
        validator_config.network_address().clone(),
        Arc::new(state),
        validator_config.consensus_config().address().clone(),
        /* tx_consensus_listener */ tx_sui_to_consensus,
    ))
}

/// Spawn all the subsystems run by a Sui authority: a consensus node, a sui authority server,
/// and a consensus listener bridging the consensus node and the sui authority.
pub async fn make_authority(
    validator_config: &ValidatorConfig,
    state: AuthorityState,
) -> SuiResult<AuthorityServer> {
    let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1_000);
    let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1_000);

    let authority_state = Arc::new(state);

    // Spawn the consensus node of this authority.
    let consensus_keypair = validator_config.key_pair().make_narwhal_keypair();
    let consensus_name = consensus_keypair.name.clone();
    let consensus_store =
        narwhal_node::NodeStorage::reopen(validator_config.consensus_config().db_path());
    narwhal_node::Node::spawn_primary(
        consensus_keypair,
        validator_config
            .committee_config()
            .narwhal_committee()
            .to_owned(),
        &consensus_store,
        validator_config
            .consensus_config()
            .narwhal_config()
            .to_owned(),
        /* consensus */ true, // Indicate that we want to run consensus.
        /* execution_state */ authority_state.clone(),
        /* tx_confirmation */ tx_consensus_to_sui,
    )
    .await?;
    narwhal_node::Node::spawn_workers(
        consensus_name,
        /* ids */ vec![0], // We run a single worker with id '0'.
        validator_config
            .committee_config()
            .narwhal_committee()
            .to_owned(),
        &consensus_store,
        validator_config
            .consensus_config()
            .narwhal_config()
            .to_owned(),
    );

    // Spawn a consensus listener. It listen for consensus outputs and notifies the
    // authority server when a sequenced transaction is ready for execution.
    ConsensusListener::spawn(
        rx_sui_to_consensus,
        rx_consensus_to_sui,
        /* max_pending_transactions */ 1_000_000,
    );

    // If we have network information make authority clients
    // to all authorities in the system.
    let _active_authority: Option<()> = {
        let mut authority_clients = BTreeMap::new();
        let mut config = mysten_network::config::Config::new();
        config.connect_timeout = Some(Duration::from_secs(5));
        config.request_timeout = Some(Duration::from_secs(5));
        for validator in validator_config.committee_config().validator_set() {
            let channel = config.connect_lazy(validator.network_address()).unwrap();
            let client = NetworkAuthorityClient::new(channel);
            authority_clients.insert(validator.public_key(), client);
        }

        let _active_authority = ActiveAuthority::new(authority_state.clone(), authority_clients)?;

        // TODO: turn on to start the active part of validators
        //
        // let join_handle = active_authority.spawn_all_active_processes().await;
        // Some(join_handle)
        None
    };

    // Return new authority server. It listen to users transactions and send back replies.
    Ok(AuthorityServer::new(
        validator_config.network_address().to_owned(),
        authority_state,
        validator_config.consensus_config().address().to_owned(),
        /* tx_consensus_listener */ tx_sui_to_consensus,
    ))
}
