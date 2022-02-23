// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use dropshot::endpoint;
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext,
};
use move_package::BuildConfig;
use serde_json::json;
use sui::config::{AccountInfo, AuthorityInfo, Config, GenesisConfig, NetworkConfig, WalletConfig};
use sui::sui_commands;
use sui::wallet_commands::WalletContext;
use sui_adapter::adapter::generate_package_id;
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{Client, ClientState};
use sui_types::base_types::*;
use sui_types::committee::Committee;
use sui_types::crypto::get_key_pair;
use sui_types::object::Object;

use futures::stream::{futures_unordered::FuturesUnordered, StreamExt as _};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use tokio::task::{self, JoinHandle};
use tracing::{error, info};

use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), String> {
    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((Ipv6Addr::LOCALHOST, 5000)),
        ..Default::default()
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("rest_server")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    tracing_subscriber::fmt().init();

    let mut api = ApiDescription::new();
    api.register(start).unwrap();
    api.register(genesis).unwrap();
    api.register(stop).unwrap();

    let api_context = ServerContext::new();

    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    server.await
}

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    genesis_config_path: String,
    wallet_config_path: String,
    network_config_path: String,
    authority_db_path: String,
    client_db_path: Arc<Mutex<String>>,
    authority_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    wallet_context: Arc<Mutex<Option<WalletContext>>>,
}

impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            genesis_config_path: String::from("genesis.conf"),
            wallet_config_path: String::from("wallet.conf"),
            network_config_path: String::from("./network.conf"),
            authority_db_path: String::from("./authorities_db"),
            client_db_path: Arc::new(Mutex::new(String::new())),
            authority_handles: Arc::new(Mutex::new(Vec::new())),
            wallet_context: Arc::new(Mutex::new(None)),
        }
    }
}

/**
 * 'GenesisResponse' returns the genesis of wallet & network config.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GenesisResponse {
    wallet_config: serde_json::Value,
    network_config: serde_json::Value,
}

/**
 * [SUI] Use to provide server configurations for genesis.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/genesis",
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    let server_context = rqctx.context();
    let genesis_config_path = &server_context.genesis_config_path;
    let network_config_path = &server_context.network_config_path;
    let wallet_config_path = &server_context.wallet_config_path;

    let mut network_config = NetworkConfig::read_or_create(&PathBuf::from(network_config_path))
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to read network config: {error}"),
            )
        })?;

    if !network_config.authorities.is_empty() {
        return Err(custom_http_error(
            hyper::StatusCode::CONFLICT,
            String::from("Cannot run genesis on a existing network, stop network to try again."),
        ));
    }

    let mut voting_right = BTreeMap::new();
    let mut authority_info = Vec::new();

    let working_dir = network_config.config_path().parent().unwrap().to_owned();
    let genesis_conf = GenesisConfig::default_genesis(&working_dir.join(genesis_config_path))
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to create default genesis configuration: {error}"),
            )
        })?;

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
        network_config.authorities.push(authority);
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
    let sui_lib = sui_framework::get_sui_framework_modules(&genesis_conf.sui_framework_lib_path)
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to get sui framework modules: {error}"),
            )
        })?;
    let lib_object =
        Object::new_package(sui_lib, SuiAddress::default(), TransactionDigest::genesis());
    preload_modules.push(lib_object);

    info!(
        "Loading Move framework lib from {:?}",
        genesis_conf.move_framework_lib_path
    );
    let move_lib = sui_framework::get_move_stdlib_modules(&genesis_conf.move_framework_lib_path)
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to get move stdlib modules: {error}"),
            )
        })?;
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
                sui_framework::build_move_package(&path, BuildConfig::default(), false).map_err(
                    |error| {
                        custom_http_error(
                            hyper::StatusCode::CONFLICT,
                            format!("Unable to build move package: {error}"),
                        )
                    },
                )?;
            generate_package_id(
                &mut modules,
                &mut TxContext::new(&SuiAddress::default(), TransactionDigest::genesis()),
            )
            .map_err(|error| {
                custom_http_error(
                    hyper::StatusCode::CONFLICT,
                    format!("Unable to generate package id: {error}"),
                )
            })?;

            let object =
                Object::new_package(modules, SuiAddress::default(), TransactionDigest::genesis());
            info!("Loaded package [{}] from {:?}.", object.id(), path);
            // Writing package id to network.conf for user to retrieve later.
            network_config
                .loaded_move_packages
                .push((path, object.id()));
            preload_modules.push(object)
        }
    }

    let committee = Committee::new(voting_right);

    // Make server state to persist the objects and modules.
    info!(
        "Preloading {} objects to authorities.",
        preload_objects.len()
    );
    for authority in &network_config.authorities {
        sui_commands::make_server(
            authority,
            &committee,
            preload_modules.clone(),
            &preload_objects,
            network_config.buffer_size,
        )
        .await
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to make server: {error}"),
            )
        })?;
    }

    info!("Network genesis completed.");
    if let Err(error) = network_config.save() {
        return Err(custom_http_error(
            hyper::StatusCode::CONFLICT,
            format!("Network config was unable to be saved: {error}"),
        ));
    };
    info!(
        "Network config file is stored in {:?}.",
        network_config.config_path()
    );

    let mut wallet_config =
        WalletConfig::create(&working_dir.join(wallet_config_path)).map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Wallet config was unable to be created: {error}"),
            )
        })?;
    wallet_config.authorities = authority_info;
    wallet_config.accounts = new_addresses;
    // Need to use a random id because rocksdb locks on current process which means even if directory is deleted
    // the lock will remain causing an IO Error
    let client_db_path = format!("client_db_{:?}", ObjectID::random());
    wallet_config.db_folder_path = working_dir.join(&client_db_path);
    *server_context.client_db_path.lock().unwrap() = client_db_path;
    if let Err(error) = wallet_config.save() {
        return Err(custom_http_error(
            hyper::StatusCode::CONFLICT,
            format!("Wallet config was unable to be saved: {error}"),
        ));
    };
    info!(
        "Wallet config file is stored in {:?}.",
        wallet_config.config_path()
    );

    Ok(HttpResponseOk(GenesisResponse {
        wallet_config: json!(wallet_config),
        network_config: json!(network_config),
    }))
}

/**
 * [SUI] Start servers with specified configurations.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/start",
}]
async fn start(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    let server_context = rqctx.context();
    let network_config_path = &server_context.network_config_path;

    let network_config = NetworkConfig::read_or_create(&PathBuf::from(network_config_path))
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to read network config: {error}"),
            )
        })?;

    if network_config.authorities.is_empty() {
        return Err(custom_http_error(
            hyper::StatusCode::CONFLICT,
            String::from("No authority configured for the network, please run genesis."),
        ));
    }

    {
        if !(*server_context.authority_handles.lock().unwrap()).is_empty() {
            return Err(custom_http_error(
                hyper::StatusCode::FORBIDDEN,
                String::from("Sui network is already running."),
            ));
        }
    }

    let committee = Committee::new(
        network_config
            .authorities
            .iter()
            .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
            .collect(),
    );
    let mut handles = FuturesUnordered::new();

    for authority in &network_config.authorities {
        let server = sui_commands::make_server(
            authority,
            &committee,
            vec![],
            &[],
            network_config.buffer_size,
        )
        .await
        .map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to make server: {error}"),
            )
        })?;
        handles.push(async move {
            match server.spawn().await {
                Ok(server) => Ok(server),
                Err(err) => {
                    return Err(custom_http_error(
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Failed to start server: {}", err),
                    ));
                }
            }
        })
    }

    let num_authorities = handles.len();
    info!("Started {} authorities", num_authorities);

    while let Some(spawned_server) = handles.next().await {
        server_context
            .authority_handles
            .lock()
            .unwrap()
            .push(task::spawn(async {
                if let Err(err) = spawned_server.unwrap().join().await {
                    error!("Server ended with an error: {}", err);
                }
            }));
    }

    let wallet_config_path = &server_context.wallet_config_path;

    let wallet_config =
        WalletConfig::read_or_create(&PathBuf::from(wallet_config_path)).map_err(|error| {
            custom_http_error(
                hyper::StatusCode::CONFLICT,
                format!("Unable to read wallet config: {error}"),
            )
        })?;

    let addresses = wallet_config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut wallet_context = WalletContext::new(wallet_config).map_err(|error| {
        custom_http_error(
            hyper::StatusCode::CONFLICT,
            format!("Can't create new wallet context: {error}"),
        )
    })?;

    // Sync all accounts.
    for address in addresses.iter() {
        let client_state = wallet_context
            .get_or_create_client_state(address)
            .map_err(|error| {
                custom_http_error(
                    hyper::StatusCode::CONFLICT,
                    format!("Can't create client state: {error}"),
                )
            })?;
        if let Some(err) = sync_client_state(client_state).await {
            return Err(err);
        }
    }

    *server_context.wallet_context.lock().unwrap() = Some(wallet_context);

    Ok(HttpResponseOk(format!(
        "Started {} authorities",
        num_authorities
    )))
}

/**
 * [SUI] Stop servers and delete storage.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/stop",
}]
async fn stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();

    for authority_handle in &*server_context.authority_handles.lock().unwrap() {
        authority_handle.abort();
    }
    (*server_context.authority_handles.lock().unwrap()).clear();

    fs::remove_dir_all(server_context.client_db_path.lock().unwrap().clone()).ok();
    fs::remove_dir_all(&server_context.authority_db_path).ok();
    fs::remove_file(&server_context.network_config_path).ok();
    fs::remove_file(&server_context.wallet_config_path).ok();

    Ok(HttpResponseUpdatedNoContent())
}

async fn sync_client_state(client_state: &mut ClientState<AuthorityClient>) -> Option<HttpError> {
    // synchronize with authorities
    let res = async move { client_state.sync_client_state().await };
    res.await.err().map(|err| {
        custom_http_error(
            hyper::StatusCode::FAILED_DEPENDENCY,
            format!("Sync error: {:?}", err),
        )
    })
}

fn custom_http_error(status_code: http::StatusCode, message: String) -> HttpError {
    HttpError::for_client_error(None, status_code, message)
}
