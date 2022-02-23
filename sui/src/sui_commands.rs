// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{
    AccountInfo, AuthorityInfo, AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig,
    WalletConfig,
};
use anyhow::anyhow;
use futures::future::join_all;
use move_package::BuildConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::StructOpt;
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_server::AuthorityServer;
use sui_types::base_types::{SequenceNumber, SuiAddress, TransactionDigest, TxContext};

use sui_adapter::adapter::generate_package_id;
use sui_types::committee::Committee;
use sui_types::crypto::get_key_pair;
use sui_types::error::SuiResult;
use sui_types::object::Object;
use tracing::{error, info};

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
                let genesis_conf = if let Some(path) = path {
                    GenesisConfig::read(path)?
                } else {
                    // Network config has been created by this point, safe to unwrap.
                    let working_dir = config.config_path().parent().unwrap();
                    GenesisConfig::default_genesis(&working_dir.join("genesis.conf"))?
                };
                // We have created the config file, safe to unwrap the path here.
                let working_dir = &config.config_path().parent().unwrap().to_path_buf();
                let wallet_path = working_dir.join("wallet.conf");
                let mut wallet_config = WalletConfig::create(&wallet_path)?;
                wallet_config.db_folder_path = working_dir.join("client_db");
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
    let mut preload_modules = Vec::new();
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
    for account in genesis_conf.accounts {
        let address = if let Some(address) = account.address {
            address
        } else {
            let (address, key_pair) = get_key_pair();
            new_addresses.push(AccountInfo { address, key_pair });
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

    // Load Sui and Move framework lib
    info!(
        "Loading Sui framework lib from {:?}",
        genesis_conf.sui_framework_lib_path
    );
    let sui_lib = sui_framework::get_sui_framework_modules(&genesis_conf.sui_framework_lib_path)?;
    let lib_object =
        Object::new_package(sui_lib, SuiAddress::default(), TransactionDigest::genesis());
    preload_modules.push(lib_object);

    info!(
        "Loading Move framework lib from {:?}",
        genesis_conf.move_framework_lib_path
    );
    let move_lib = sui_framework::get_move_stdlib_modules(&genesis_conf.move_framework_lib_path)?;
    let lib_object = Object::new_package(
        move_lib,
        SuiAddress::default(),
        TransactionDigest::genesis(),
    );
    preload_modules.push(lib_object);

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
            generate_package_id(
                &mut modules,
                &mut TxContext::new(&SuiAddress::default(), TransactionDigest::genesis()),
            )?;

            let object =
                Object::new_package(modules, SuiAddress::default(), TransactionDigest::genesis());
            info!("Loaded package [{}] from {:?}.", object.id(), path);
            // Writing package id to network.conf for user to retrieve later.
            config.loaded_move_packages.push((path, object.id()));
            preload_modules.push(object)
        }
    }

    let committee = Committee::new(voting_right);

    // Make server state to persist the objects and modules.
    info!(
        "Preloading {} objects to authorities.",
        preload_objects.len()
    );
    for authority in &config.authorities {
        make_server(
            authority,
            &committee,
            preload_modules.clone(),
            &preload_objects,
            config.buffer_size,
        )
        .await?;
    }
    wallet_config.authorities = authority_info;
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
    preload_modules: Vec<Object>,
    preload_objects: &[Object],
    buffer_size: usize,
) -> SuiResult<AuthorityServer> {
    let store = Arc::new(AuthorityStore::open(&authority.db_path, None));
    let name = *authority.key_pair.public_key_bytes();

    let state = AuthorityState::new(
        committee.clone(),
        name,
        Box::pin(authority.key_pair.copy()),
        store,
        preload_modules,
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
