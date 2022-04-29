// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{make_default_narwhal_committee, AuthorityInfo, CONSENSUS_DB_NAME};
use crate::config::{
    AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig, PersistedConfig, WalletConfig,
};
use crate::gateway_config::{GatewayConfig, GatewayType};
use crate::keystore::{Keystore, KeystoreType, SuiKeystore};
use crate::{sui_config_dir, SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG, SUI_WALLET_CONFIG};
use anyhow::{anyhow, bail};
use base64ct::{Base64, Encoding};
use clap::*;
use futures::future::join_all;
use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use narwhal_config::{Committee as ConsensusCommittee, Parameters as ConsensusParameters};
use narwhal_crypto::ed25519::Ed25519PublicKey;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_adapter::adapter::generate_package_id;
use sui_adapter::genesis;
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_active::ActiveAuthority;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::authority_server::AuthorityServer;
use sui_core::consensus_adapter::ConsensusListener;
use sui_network::network::NetworkClient;
use sui_network::transport::SpawnedServer;
use sui_network::transport::DEFAULT_MAX_DATAGRAM_SIZE;
use sui_types::base_types::decode_bytes_hex;
use sui_types::base_types::encode_bytes_hex;
use sui_types::base_types::{SequenceNumber, SuiAddress, TxContext};
use sui_types::committee::Committee;
use sui_types::error::SuiResult;
use sui_types::object::Object;
use tokio::sync::mpsc::channel;
use tracing::{error, info};

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
                let config_path = config
                    .clone()
                    .unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let config: NetworkConfig = PersistedConfig::read(&config_path).map_err(|err| {
                    err.context(format!(
                        "Cannot open Sui network config file at {:?}",
                        config_path
                    ))
                })?;

                // Start a sui validator (including its consensus node).
                SuiNetwork::start(&config)
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
                    for auth in config.authorities.iter() {
                        let addr = SuiAddress::from(auth.key_pair.public_key_bytes());
                        println!("{}:{} - {}", auth.host, auth.port, addr);
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
                    None => GenesisConfig::default_genesis(sui_config_dir)?,
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

                GatewayConfig {
                    db_folder_path: gateway_db_folder_path,
                    authorities: network_config.get_authority_infos(),
                    ..Default::default()
                }
                .persisted(&gateway_path)
                .save()?;
                info!("Gateway config file is stored in {:?}.", gateway_path);

                let wallet_gateway_config = GatewayConfig {
                    db_folder_path,
                    authorities: network_config.get_authority_infos(),
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
    pub spawned_authorities: Vec<SpawnedServer<AuthorityServer>>,
}

impl SuiNetwork {
    pub async fn start(config: &NetworkConfig) -> Result<Self, anyhow::Error> {
        if config.authorities.is_empty() {
            return Err(anyhow!(
                "No authority configured for the network, please run genesis."
            ));
        }
        info!(
            "Starting network with {} authorities",
            config.authorities.len()
        );

        let committee = Committee::new(
            config.epoch,
            config
                .authorities
                .iter()
                .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
                .collect(),
        );

        let consensus_committee = make_default_narwhal_committee(&config.authorities)?;
        let consensus_parameters = ConsensusParameters::default();

        // Pass in the newtwork parameters of all authorities
        let net = config.get_authority_infos();

        let mut spawned_authorities = Vec::new();
        for authority in &config.authorities {
            let consensus_store_path = sui_config_dir()?
                .join(CONSENSUS_DB_NAME)
                .join(encode_bytes_hex(authority.key_pair.public_key_bytes()));

            let server = make_server(
                authority,
                &committee,
                config.buffer_size,
                &consensus_committee,
                &consensus_store_path,
                &consensus_parameters,
                Some(net.clone()),
            )
            .await?;
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
    info!(
        "Creating {} new authorities...",
        genesis_conf.authorities.len()
    );

    let mut network_config = NetworkConfig {
        epoch: 0,
        authorities: vec![],
        buffer_size: DEFAULT_MAX_DATAGRAM_SIZE,
        loaded_move_packages: vec![],
    };
    let mut voting_right = BTreeMap::new();
    for authority in genesis_conf.authorities {
        voting_right.insert(*authority.key_pair.public_key_bytes(), authority.stake);
        network_config.authorities.push(authority);
    }

    let mut addresses = Vec::new();
    let mut preload_modules: Vec<Vec<CompiledModule>> = Vec::new();
    let mut preload_objects = Vec::new();

    info!("Creating accounts and gas objects...",);

    let mut keystore = SuiKeystore::default();
    for account in genesis_conf.accounts {
        let address = if let Some(address) = account.address {
            address
        } else {
            keystore.add_random_key()?
        };

        addresses.push(address);

        for object_conf in account.gas_objects {
            let new_object = Object::with_id_owner_gas_coin_object_for_testing(
                object_conf.object_id,
                SequenceNumber::new(),
                address,
                object_conf.gas_value,
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
            network_config.loaded_move_packages.push((path, package_id));
            preload_modules.push(modules)
        }
    }

    let committee = Committee::new(network_config.epoch, voting_right);
    for authority in &network_config.authorities {
        make_server_with_genesis_ctx(
            authority,
            &committee,
            preload_modules.clone(),
            &preload_objects,
            network_config.buffer_size,
            &mut genesis_ctx.clone(),
        )
        .await?;
    }

    Ok((network_config, addresses, keystore))
}

pub async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    buffer_size: usize,
    consensus_committee: &ConsensusCommittee<Ed25519PublicKey>,
    consensus_store_path: &Path,
    consensus_parameters: &ConsensusParameters,
    net_parameters: Option<Vec<AuthorityInfo>>,
) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(&authority.db_path, None));
    let name = *authority.key_pair.public_key_bytes();
    let state = AuthorityState::new_without_genesis(
        committee.clone(),
        name,
        Arc::pin(authority.key_pair.copy()),
        store,
    )
    .await;

    make_authority(
        authority,
        buffer_size,
        state,
        consensus_committee,
        consensus_store_path,
        consensus_parameters,
        net_parameters,
    )
    .await
}

async fn make_server_with_genesis_ctx(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    buffer_size: usize,
    genesis_ctx: &mut TxContext,
) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(&authority.db_path, None));
    let name = *authority.key_pair.public_key_bytes();

    let state = AuthorityState::new(
        committee.clone(),
        name,
        Arc::pin(authority.key_pair.copy()),
        store,
        preload_modules,
        genesis_ctx,
    )
    .await;

    for object in preload_objects {
        state.insert_genesis_object(object.clone()).await;
    }

    let (tx_sui_to_consensus, _rx_sui_to_consensus) = channel(1);
    Ok(AuthorityServer::new(
        authority.host.clone(),
        authority.port,
        buffer_size,
        Arc::new(state),
        authority.consensus_address,
        /* tx_consensus_listener */ tx_sui_to_consensus,
    ))
}

/// Spawn all the subsystems run by a Sui authority: a consensus node, a sui authority server,
/// and a consensus listener bridging the consensus node and the sui authority.
pub async fn make_authority(
    authority: &AuthorityPrivateInfo,
    buffer_size: usize,
    state: AuthorityState,
    consensus_committee: &ConsensusCommittee<Ed25519PublicKey>,
    consensus_store_path: &Path,
    consensus_parameters: &ConsensusParameters,
    net_parameters: Option<Vec<AuthorityInfo>>,
) -> SuiResult<AuthorityServer> {
    let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1_000);
    let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1_000);

    let authority_state = Arc::new(state);

    // Spawn the consensus node of this authority.
    let consensus_keypair = authority.key_pair.make_narwhal_keypair();
    let consensus_name = consensus_keypair.name.clone();
    let consensus_store = narwhal_node::NodeStorage::reopen(consensus_store_path);
    narwhal_node::Node::spawn_primary(
        consensus_keypair,
        consensus_committee.clone(),
        &consensus_store,
        consensus_parameters.clone(),
        /* consensus */ true, // Indicate that we want to run consensus.
        /* execution_state */ authority_state.clone(),
        /* tx_confirmation */ tx_consensus_to_sui,
    )
    .await?;
    narwhal_node::Node::spawn_workers(
        consensus_name,
        /* ids */ vec![0], // We run a single worker with id '0'.
        consensus_committee.clone(),
        &consensus_store,
        consensus_parameters.clone(),
    );

    // Spawn a consensus listener. It listen for consensus outputs and notifies the
    // authority server when a sequenced transaction is ready for execution.
    ConsensusListener::spawn(
        authority_state.clone(),
        rx_sui_to_consensus,
        rx_consensus_to_sui,
    );

    // If we have network information make authority clients
    // to all authorities in the system.
    let _active_authority = if let Some(network) = net_parameters {
        let mut authority_clients = BTreeMap::new();
        for info in &network {
            let client = NetworkAuthorityClient::new(NetworkClient::new(
                info.host.clone(),
                info.base_port,
                buffer_size,
                Duration::from_secs(5),
                Duration::from_secs(5),
            ));
            authority_clients.insert(info.name, client);
        }

        let active_authority = ActiveAuthority::new(authority_state.clone(), authority_clients)?;

        let join_handle = active_authority.spawn_all_active_processes().await;
        Some(join_handle)
    } else {
        None
    };

    // Return new authority server. It listen to users transactions and send back replies.
    Ok(AuthorityServer::new(
        authority.host.clone(),
        authority.port,
        buffer_size,
        authority_state,
        authority.consensus_address,
        /* tx_consensus_listener */ tx_sui_to_consensus,
    ))
}
