// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crossbeam::thread as cb_thread;

use dropshot::endpoint;
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext, TypedBody,
};

use serde_json::json;
use sui::config::{
    AccountInfo, AuthorityInfo, AuthorityPrivateInfo, NetworkConfig, PortAllocator, WalletConfig,
};
use sui::utils::Config;
use sui::wallet_commands::WalletContext;
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_client::AuthorityClient;
use sui_core::authority_server::AuthorityServer;
use sui_core::client::{Client, ClientState};
use sui_types::base_types::*;
use sui_types::committee::Committee;
use sui_types::object::Object as SuiObject;

use futures::future::join_all;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use tracing::error;

use std::sync::{Arc, Mutex};
use std::thread;

const DEFAULT_WEIGHT: usize = 1;

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
    wallet_config_path: String,
    network_config_path: String,
    authority_db_path: String,
    client_db_path: String,
    server_lock: Arc<AtomicBool>,
    wallet_context: Arc<Mutex<Option<WalletContext>>>,
}

impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            // TODO: Make configurable (will look similar to fastnft/issues/400)
            wallet_config_path: String::from("./wallet.conf"),
            network_config_path: String::from("./network.conf"),
            authority_db_path: String::from("./authorities_db"),
            client_db_path: String::from("./client_db"),
            server_lock: Arc::new(AtomicBool::new(false)),
            wallet_context: Arc::new(Mutex::new(None)),
        }
    }
}

/**
* 'GenesisRequest' represents the server configuration.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GenesisRequest {
    num_authorities: Option<u16>,
    num_objects: Option<u16>,
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
    path = "/sui/genesis",
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<GenesisRequest>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    let server_context = rqctx.context();
    let network_config_path = &server_context.network_config_path;
    let wallet_config_path = &server_context.wallet_config_path;

    let genesis_params = request.into_inner();
    let num_authorities = genesis_params.num_authorities.unwrap_or(4);
    let num_objects = genesis_params.num_objects.unwrap_or(5);

    let mut network_config =
        match NetworkConfig::read_or_create(&PathBuf::from(network_config_path)) {
            Ok(network_config) => network_config,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::CONFLICT,
                    format!("Unable to read network config: {error}"),
                ))
            }
        };

    if !network_config.authorities.is_empty() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::CONFLICT,
            String::from("Cannot run genesis on a existing network, stop network to try again."),
        ));
    }

    let mut authorities = BTreeMap::new();
    let mut authority_info = Vec::new();
    let mut port_allocator = PortAllocator::new(10000);

    for _ in 0..num_authorities {
        let (address, key_pair) = get_key_pair();
        let info = AuthorityPrivateInfo {
            address,
            key_pair,
            host: "127.0.0.1".to_string(),
            port: match port_allocator.next_port() {
                Some(port) => port,
                None => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::CONFLICT,
                        String::from(
                            "Could not create authority beacause there were no free ports",
                        ),
                    ))
                }
            },
            db_path: PathBuf::from(format!("./authorities_db/{:?}", address)),
        };
        authority_info.push(AuthorityInfo {
            address,
            host: info.host.clone(),
            base_port: info.port,
        });
        authorities.insert(info.address, 1);
        network_config.authorities.push(info);
    }

    match network_config.save() {
        Ok(_) => (),
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Network config was unable to be saved: {error}"),
            ))
        }
    };

    let mut new_addresses = Vec::new();
    let mut preload_objects: Vec<SuiObject> = Vec::new();

    for _ in 0..num_objects {
        let (address, key_pair) = get_key_pair();
        new_addresses.push(AccountInfo { address, key_pair });
        for _ in 0..num_objects {
            let new_object = SuiObject::with_id_owner_gas_coin_object_for_testing(
                ObjectID::random(),
                SequenceNumber::new(),
                address,
                1000000,
            );
            preload_objects.push(new_object);
        }
    }
    let committee = Committee::new(authorities);

    // Make server state to persist the objects.
    for authority in network_config.authorities.iter() {
        make_server(
            authority,
            &committee,
            &preload_objects,
            network_config.buffer_size,
        )
        .await;
    }

    let mut wallet_config = match WalletConfig::create(&PathBuf::from(wallet_config_path)) {
        Ok(wallet_config) => wallet_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Wallet config was unable to be created: {error}"),
            ))
        }
    };
    wallet_config.authorities = authority_info;
    wallet_config.accounts = new_addresses;
    match wallet_config.save() {
        Ok(_) => (),
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Wallet config was unable to be saved: {error}"),
            ))
        }
    };

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
    path = "/sui/start",
}]
async fn start(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    let server_context = rqctx.context();
    let network_config_path = &server_context.network_config_path;

    let network_config = match NetworkConfig::read_or_create(&PathBuf::from(network_config_path)) {
        Ok(network_config) => network_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Unable to read network config: {error}"),
            ))
        }
    };

    if network_config.authorities.is_empty() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::CONFLICT,
            String::from("No authority configured for the network, please run genesis."),
        ));
    }

    if server_context.server_lock.load(Ordering::SeqCst) {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FORBIDDEN,
            String::from("Sui network is already running."),
        ));
    }

    let committee = Committee::new(
        network_config
            .authorities
            .iter()
            .map(|info| (info.address, DEFAULT_WEIGHT))
            .collect(),
    );
    let mut handles = Vec::new();
    let rt = Runtime::new().unwrap();

    for authority in network_config.authorities {
        let server = make_server(&authority, &committee, &[], network_config.buffer_size).await;

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
        })
    }

    let num_authorities = handles.len();

    server_context.server_lock.store(true, Ordering::SeqCst);
    thread::spawn({
        move || {
            rt.block_on(join_all(handles));
        }
    });

    let wallet_config_path = &server_context.wallet_config_path;

    let config = match WalletConfig::read_or_create(&PathBuf::from(wallet_config_path)) {
        Ok(network_config) => network_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Unable to read wallet config: {error}"),
            ))
        }
    };
    let addresses = config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut wallet_context = match WalletContext::new(config) {
        Ok(wallet_context) => wallet_context,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::CONFLICT,
                format!("Can't create new wallet context: {error}"),
            ))
        }
    };

    // Sync all accounts.
    for address in addresses.iter() {
        let client_state = match wallet_context.get_or_create_client_state(address) {
            Ok(client_state) => client_state,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::CONFLICT,
                    format!("Can't create client state: {error}"),
                ))
            }
        };
        if let Some(err) = sync_client_state(client_state) {
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
    path = "/sui/stop",
}]
async fn stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();

    // TODO: Figure out how kill thread that is hosting the authorities.
    // server_context.server_lock.store(false, Ordering::SeqCst);

    fs::remove_dir_all(&server_context.client_db_path).ok();
    fs::remove_dir_all(&server_context.authority_db_path).ok();
    fs::remove_file(&server_context.network_config_path).ok();
    fs::remove_file(&server_context.wallet_config_path).ok();

    Ok(HttpResponseUpdatedNoContent())
}

fn sync_client_state(client_state: &mut ClientState<AuthorityClient>) -> Option<HttpError> {
    match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // synchronize with authorities
                let rt = Runtime::new().unwrap();
                rt.block_on(async move { client_state.sync_client_state().await })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => match result {
                Ok(_) => None,
                Err(err) => Some(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Sync error: {err}"),
                )),
            },
            Err(err) => Some(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Sync error: {:?}", err),
            )),
        },
        Err(err) => Some(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            format!("Sync error: {:?}", err),
        )),
    }
}

async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    pre_load_objects: &[SuiObject],
    buffer_size: usize,
) -> AuthorityServer {
    let path = authority.db_path.clone();
    let store = Arc::new(AuthorityStore::open(path, None));

    let state = AuthorityState::new_with_genesis_modules(
        committee.clone(),
        authority.address,
        Box::pin(authority.key_pair.copy()),
        store,
    )
    .await;

    for object in pre_load_objects {
        state.init_order_lock(object.to_object_reference()).await;
        state.insert_object(object.clone()).await;
    }

    AuthorityServer::new(authority.host.clone(), authority.port, buffer_size, state)
}
