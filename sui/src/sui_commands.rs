// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{
    AuthorityInfo, AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig, WalletConfig,
};
use crate::keystore::KeystoreType;
use anyhow::anyhow;
use futures::future::join_all;
use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;
use structopt::StructOpt;
use sui_adapter::adapter::generate_package_id;
use sui_adapter::genesis;
use sui_core::authority::spawn_authority;
use sui_types::base_types::{SequenceNumber, TxContext};
use sui_types::committee::Committee;
use sui_types::crypto::get_key_pair;
use sui_types::object::Object;
use tokio::sync::mpsc::channel;
use tracing::info;

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
                wallet_config.db_folder_path = working_dir.join("client_db");
                wallet_config.keystore = KeystoreType::File(working_dir.join("wallet.key"));
                genesis(config, genesis_conf, &mut wallet_config).await
            }
        }
    }
}

async fn start_network(config: &mut NetworkConfig) -> Result<(), anyhow::Error> {
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

    while let Some(authority) = config.authorities.pop() {
        let committee = committee.clone();
        let handle = tokio::spawn(async move {
            make_server(&authority, committee, vec![], &[]).await;
        });
        handles.push(handle);
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
            address
        } else {
            let (address, key_pair) = get_key_pair();
            new_addresses.push(address);
            keystore.add_key(key_pair)?;
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

    config.save()?;
    info!(
        "Network config file is stored in {:?}.",
        config.config_path()
    );

    /*
    // Spawning the authorities is only required to create the database folder.
    let committee = Committee::new(voting_right);
    while let Some(authority) = config.authorities.pop() {
        let committee = committee.clone();
        let preload_modules = preload_modules.clone();
        let preload_objects = preload_objects.clone();
        let mut genesis_ctx = genesis_ctx.clone();
        tokio::spawn(async move {
            make_server_with_genesis_ctx(
                &authority,
                committee,
                preload_modules,
                &preload_objects,
                &mut genesis_ctx,
            )
            .await;
        });
    }
    tokio::task::yield_now().await;
    */

    wallet_config.authorities = authority_info;
    wallet_config.accounts = new_addresses;

    info!("Network genesis completed.");
    wallet_config.save()?;
    info!(
        "Wallet config file is stored in {:?}.",
        wallet_config.config_path()
    );
    Ok(())
}

pub async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: Committee,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
) {
    let mut genesis_ctx = genesis::get_genesis_context();
    make_server_with_genesis_ctx(
        authority,
        committee,
        preload_modules,
        preload_objects,
        &mut genesis_ctx,
    )
    .await
}

async fn make_server_with_genesis_ctx(
    authority: &AuthorityPrivateInfo,
    committee: Committee,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    genesis_ctx: &mut TxContext,
) {
    // The sender part of this channel goes to the sequencer (TODO).
    let (_tx_consensus, rx_consensus) = channel(1_000);

    // Extract the network address of this authority from its configs.
    let address = format!("{}:{}", authority.host, authority.port)
        .parse()
        .unwrap();

    // Spawn the authority.
    spawn_authority(
        &authority.key_pair,
        committee,
        &authority.db_path.as_path().display().to_string(),
        address,
        preload_modules,
        preload_objects,
        genesis_ctx,
        rx_consensus,
    )
    .await;
}
