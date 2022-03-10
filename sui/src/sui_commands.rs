// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use futures::future::join_all;
use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use structopt::StructOpt;
use tracing::{error, info};

use sui_adapter::adapter::generate_package_id;
use sui_adapter::genesis;
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_server::AuthorityServer;
use sui_types::base_types::{SequenceNumber, TxContext};
use sui_types::committee::Committee;
use sui_types::error::SuiResult;
use sui_types::object::Object;

use crate::config::{
    AuthorityInfo, AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig, WalletConfig,
};
use crate::gateway::{EmbeddedGatewayConfig, GatewayType};
use crate::keystore::KeystoreType;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum SuiCommand {
    /// Start sui network.
    #[structopt(name = "start")]
    Start,
    #[structopt(name = "genesis")]
    Genesis {
        #[structopt(long)]
        config: Option<PathBuf>,
    },
}

impl SuiCommand {
    pub async fn execute(&self, config: &mut NetworkConfig) -> Result<(), anyhow::Error> {
        match self {
            SuiCommand::Start => start_network(config).await,
            SuiCommand::Genesis { config: path } => {
                // Network config has been created by this point, safe to unwrap.
                let working_dir = config.config_path().parent().unwrap();
                let genesis_conf = if let Some(path) = path {
                    GenesisConfig::read(path)?
                } else {
                    GenesisConfig::default_genesis(&working_dir.join("genesis.conf"))?
                };
                let wallet_path = working_dir.join("wallet.conf");
                let mut wallet_config = WalletConfig::create(&wallet_path)?;
                wallet_config.keystore = KeystoreType::File(working_dir.join("wallet.key"));
                wallet_config.gateway = GatewayType::Embedded(EmbeddedGatewayConfig {
                    db_folder_path: working_dir.join("client_db"),
                    ..Default::default()
                });
                genesis(config, genesis_conf, &mut wallet_config).await
            }
        }
    }
}

async fn start_network(config: &NetworkConfig) -> Result<(), anyhow::Error> {
    if config.authorities.is_empty() {
        return Err(anyhow!(
            "No authority configured for the network, please run genesis."
        ));
    }
    info!(
        "Starting network with {} authorities",
        config.authorities.len()
    );
    let mut handles = Vec::new();

    let committee = Committee::new(
        config
            .authorities
            .iter()
            .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
            .collect(),
    );

    for authority in &config.authorities {
        let server = make_server(authority, &committee, vec![], &[], config.buffer_size).await?;

        handles.push(async move {
            let spawned_server = match server.spawn().await {
                Ok(server) => server,
                Err(err) => {
                    error!("Failed to start server: {}", err);
                    return;
                }
            };
            if let Err(err) = spawned_server.join().await {
                error!("Server ended with an error: {}", err);
            }
        });
    }

    info!("Started {} authorities", handles.len());
    join_all(handles).await;
    info!("All server stopped.");
    Ok(())
}

pub async fn genesis(
    config: &mut NetworkConfig,
    genesis_conf: GenesisConfig,
    wallet_config: &mut WalletConfig,
) -> Result<(), anyhow::Error> {
    if !config.authorities.is_empty() {
        return Err(anyhow!("Cannot run genesis on a existing network, please delete network config file and try again."));
    }

    let mut voting_right = BTreeMap::new();
    let mut authority_info = Vec::new();

    info!(
        "Creating {} new authorities...",
        genesis_conf.authorities.len()
    );

    for authority in genesis_conf.authorities {
        voting_right.insert(*authority.key_pair.public_key_bytes(), authority.stake);
        authority_info.push(AuthorityInfo {
            name: *authority.key_pair.public_key_bytes(),
            host: authority.host.clone(),
            base_port: authority.port,
        });
        config.authorities.push(authority);
    }

    let mut new_addresses = Vec::new();
    let mut preload_modules: Vec<Vec<CompiledModule>> = Vec::new();
    let mut preload_objects = Vec::new();

    let new_account_count = genesis_conf
        .accounts
        .iter()
        .filter(|acc| acc.address.is_none())
        .count();

    info!(
        "Creating {} account(s) and gas objects...",
        new_account_count
    );

    let mut keystore = wallet_config.keystore.init()?;

    for account in genesis_conf.accounts {
        let address = if let Some(address) = account.address {
            new_addresses.push(address);
            address
        } else {
            let address = keystore.add_random_key()?;
            new_addresses.push(address);
            address
        };
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

    let mut genesis_ctx = genesis::get_genesis_context();
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
            // Writing package id to network.conf for user to retrieve later.
            config.loaded_move_packages.push((path, package_id));
            preload_modules.push(modules)
        }
    }

    let committee = Committee::new(voting_right);
    for authority in &config.authorities {
        make_server_with_genesis_ctx(
            authority,
            &committee,
            preload_modules.clone(),
            &preload_objects,
            config.buffer_size,
            &mut genesis_ctx.clone(),
        )
        .await?;
    }

    if let GatewayType::Embedded(config) = &wallet_config.gateway {
        wallet_config.gateway = GatewayType::Embedded(EmbeddedGatewayConfig {
            db_folder_path: config.db_folder_path.clone(),
            authorities: authority_info,
            ..Default::default()
        });
    }

    wallet_config.accounts = new_addresses;

    info!("Network genesis completed.");
    config.save()?;
    info!(
        "Network config file is stored in {:?}.",
        config.config_path()
    );
    wallet_config.save()?;
    info!(
        "Wallet config file is stored in {:?}.",
        wallet_config.config_path()
    );
    Ok(())
}

pub async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    buffer_size: usize,
) -> SuiResult<AuthorityServer> {
    let mut genesis_ctx = genesis::get_genesis_context();
    make_server_with_genesis_ctx(
        authority,
        committee,
        preload_modules,
        preload_objects,
        buffer_size,
        &mut genesis_ctx,
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
        state
            .init_transaction_lock(object.to_object_reference())
            .await;
        state.insert_object(object.clone()).await;
    }

    Ok(AuthorityServer::new(
        authority.host.clone(),
        authority.port,
        buffer_size,
        state,
    ))
}
