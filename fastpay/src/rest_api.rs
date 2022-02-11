// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crossbeam::thread as cb_thread;

use dropshot::endpoint;
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext, TypedBody,
};

use fastpay::config::{
    AccountInfo, AuthorityInfo, AuthorityPrivateInfo, NetworkConfig, PortAllocator, WalletConfig,
};
use fastpay::utils::Config;
use fastpay::wallet_commands::WalletContext;
use fastpay_core::authority::{AuthorityState, AuthorityStore};
use fastpay_core::authority_server::AuthorityServer;
use fastpay_core::client::Client;
use fastx_types::committee::Committee;
use fastx_types::messages::{ExecutionStatus, OrderEffects};
use fastx_types::object::Object as FastxObject;
use fastx_types::{base_types::*, object::ObjectRead};

use futures::future::join_all;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::parser::{parse_transaction_argument, parse_type_tag};
use move_core_types::transaction_argument::convert_txn_args;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
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
        .to_logger("rest-api")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    let mut api = ApiDescription::new();
    api.register(start).unwrap();
    api.register(genesis).unwrap();
    api.register(stop).unwrap();
    api.register(get_addresses).unwrap();
    api.register(get_objects).unwrap();
    api.register(get_object_info).unwrap();
    api.register(transfer_object).unwrap();
    api.register(sync).unwrap();
    api.register(publish).unwrap();
    api.register(call).unwrap();

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
    authority_db_path: Arc<Mutex<String>>,
    wallet_config_path: Arc<Mutex<String>>,
    network_config_path: Arc<Mutex<String>>,
    wallet_context: Arc<Mutex<Option<WalletContext>>>,
}

impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            wallet_config_path: Arc::new(Mutex::new(String::from("./wallet.conf"))),
            network_config_path: Arc::new(Mutex::new(String::from("./network.conf"))),
            authority_db_path: Arc::new(Mutex::new(String::from("./authorities_db"))),
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
    wallet_config: String,
    network_config: String,
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
    let network_config_path = server_context.network_config_path.lock().unwrap().clone();
    let wallet_config_path = server_context.wallet_config_path.lock().unwrap().clone();

    let genesis_params = request.into_inner();
    let num_authorities = genesis_params.num_authorities.unwrap_or(4);
    let num_objects = genesis_params.num_objects.unwrap_or(5);

    let mut network_config = match NetworkConfig::read_or_create(&network_config_path) {
        Ok(network_config) => network_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Unable to read network config: {error}"),
            ))
        }
    };

    if !network_config.authorities.is_empty() {
        return Err(
            HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                String::from("Cannot run genesis on a existing network, please delete network config file and try again.")));
    }

    let mut authorities = BTreeMap::new();
    let mut authority_info = Vec::new();
    let mut port_allocator = PortAllocator::new(10000);

    println!("Creating new authorities...");
    for _ in 0..num_authorities {
        let (address, key_pair) = get_key_pair();
        let info = AuthorityPrivateInfo {
            address,
            key_pair,
            host: "127.0.0.1".to_string(),
            port: port_allocator.next_port().expect("No free ports"), // TODO: change to http error
            db_path: format!("./authorities_db/{:?}", address),
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
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Network config was unable to be saved: {error}"),
            ))
        }
    };

    let mut new_addresses = Vec::new();
    let mut preload_objects: Vec<FastxObject> = Vec::new();

    println!("Creating test objects...");
    for _ in 0..num_objects {
        let (address, key_pair) = get_key_pair();
        new_addresses.push(AccountInfo { address, key_pair });
        for _ in 0..num_objects {
            let new_object = FastxObject::with_id_owner_gas_coin_object_for_testing(
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
    let network_config_path = network_config.config_path().to_string();
    for authority in network_config.authorities.iter() {
        make_server(
            authority,
            &committee,
            &preload_objects,
            network_config.buffer_size,
        )
        .await;
    }

    let mut wallet_config = match WalletConfig::create(&wallet_config_path) {
        Ok(wallet_config) => wallet_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
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
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Wallet config was unable to be saved: {error}"),
            ))
        }
    };

    println!("Network genesis completed.");
    println!("Network config file is stored in {}.", network_config_path);
    println!(
        "Wallet config file is stored in {}.",
        wallet_config.config_path()
    );

    // TODO: Print out the contents of wallet_config/network_config? Gated by request param
    let wallet_config_string = format!(
        "Wallet Config was created with {} accounts",
        wallet_config.accounts.len(),
    );
    let network_config_string = format!(
        "Network Config was created with {} authorities",
        network_config.authorities.len(),
    );

    Ok(HttpResponseOk(GenesisResponse {
        wallet_config: wallet_config_string,
        network_config: network_config_string,
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
    let network_config_path = server_context.network_config_path.lock().unwrap().clone();

    let network_config = match NetworkConfig::read_or_create(&network_config_path) {
        Ok(network_config) => network_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Unable to read network config: {error}"),
            ))
        }
    };

    if network_config.authorities.is_empty() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            String::from("No authority configured for the network, please run genesis."),
        ));
    }

    // TODO: check if thread/network is already before proceeding (use mutex to indicate if server has been started)

    println!(
        "Starting network with {} authorities",
        network_config.authorities.len()
    );

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
            // TODO: bubble up async server errors as a http error
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

    thread::spawn(move || {
        rt.block_on(join_all(handles));
    });

    let wallet_config_path = server_context.wallet_config_path.lock().unwrap().clone();

    let config = match WalletConfig::read_or_create(&wallet_config_path) {
        Ok(network_config) => network_config,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Unable to read wallet config: {error}"),
            ))
        }
    };
    let addresses = config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut wallet_context = WalletContext::new(config);

    // Sync all accounts.
    for address in addresses.iter() {
        let client_state = match wallet_context.get_or_create_client_state(address) {
            Ok(client_state) => client_state,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Can't create client state: {error}"),
                ))
            }
        };
        match client_state.sync_client_state().await {
            Ok(_) => (),
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Sync failed: {err}"),
                ))
            }
        };
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
    let network_config_path = server_context.network_config_path.lock().unwrap().clone();
    let wallet_config_path = server_context.wallet_config_path.lock().unwrap().clone();
    let authority_db_path = server_context.authority_db_path.lock().unwrap().clone();

    // TODO: kill thread that is hosting the authorities
    // TODO: get client db path from server context & error handle
    fs::remove_dir_all("./client_db").ok();
    fs::remove_dir_all(authority_db_path).ok();
    fs::remove_file(network_config_path).ok();
    fs::remove_file(wallet_config_path).ok();

    Ok(HttpResponseUpdatedNoContent())
}

/**
 * `GetAddressResponse` represents the list of managed accounts for this client
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetAddressResponse {
    addresses: Vec<String>,
}

/**
 * [WALLET] Retrieve all managed accounts.
 */
#[endpoint {
    method = GET,
    path = "/wallet/addresses",
}]
async fn get_addresses(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GetAddressResponse>, HttpError> {
    let server_context = rqctx.context();
    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();

    let addresses = wallet_context
        .config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();

    // Sync all accounts.
    for address in addresses.iter() {
        let client_state = match wallet_context.get_or_create_client_state(address) {
            Ok(client_state) => client_state,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Can't create client state: {error}"),
                ))
            }
        };

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
                    Ok(_) => (),
                    Err(err) => {
                        return Err(HttpError::for_client_error(
                            None,
                            hyper::StatusCode::FAILED_DEPENDENCY,
                            format!("Sync error: {err}"),
                        ))
                    }
                },
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Sync error: {:?}", err),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Sync error: {:?}", err),
                ))
            }
        };
    }

    // TODO: check if we should remove 'k#' as part of address
    Ok(HttpResponseOk(GetAddressResponse {
        addresses: addresses
            .into_iter()
            .map(|address| format!("{:?}", address))
            .collect(),
    }))
}

/**
* 'GetObjectsRequest' represents the request to get objects for an address.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectsRequest {
    address: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct Object {
    object_id: String,
    object_ref: String,
}

/**
 * 'GetObjectsResponse' is a collection of objects owned by an address.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectsResponse {
    objects: Vec<Object>,
}

/**
 * [WALLET] Return all objects owned by the account address.
 */
#[endpoint {
    method = GET,
    path = "/wallet/objects",
}]
async fn get_objects(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<GetObjectsRequest>,
) -> Result<HttpResponseOk<GetObjectsResponse>, HttpError> {
    let server_context = rqctx.context();

    let get_objects_params = request.into_inner();
    let address = get_objects_params.address;

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();

    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }

    let wallet_context = wallet_context.as_mut().unwrap();

    let client_state = match wallet_context.get_or_create_client_state(
        match &decode_address_hex(address.as_str()) {
            Ok(address) => address,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Could not decode address from hex {error}"),
                ))
            }
        },
    ) {
        Ok(client_state) => client_state,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not get or create client state: {error}"),
            ))
        }
    };
    let object_refs = client_state.object_refs();
    println!("Showing {} results.", object_refs.len());

    Ok(HttpResponseOk(GetObjectsResponse {
        objects: object_refs
            .into_iter()
            .map(|e| Object {
                object_id: e.0.to_string(),
                object_ref: format!("{:?}", e.1),
            })
            .collect::<Vec<Object>>(),
    }))
}

/**
* `GetObjectInfoRequest` represents the request to get object info.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectInfoRequest {
    object_id: String,
}

/**
* 'ObjectInfoResponse' represents the object info on the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct ObjectInfoResponse {
    owner: String,
    version: String,
    id: String,
    readonly: String,
    obj_type: String,
}

/**
 * [WALLET] Get object info.
 */
#[endpoint {
    method = GET,
    path = "/wallet/object_info",
}]
async fn get_object_info(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    let server_context = rqctx.context();

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();
    let object_id = match AccountAddress::try_from(request.into_inner().object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    // Pick the first (or any) account for use in finding obj info
    let account = match wallet_context.find_owner(&object_id) {
        Ok(account) => account,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    // Fetch the object ref
    let client_state = match wallet_context.get_or_create_client_state(&account) {
        Ok(client_state) => client_state,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not get client state for account {:?}: {error}",
                    account
                ),
            ))
        }
    };

    let object = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Get the object info
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Ok(ObjectRead::Exists(_, object)) =
                        client_state.get_object_info(object_id).await
                    {
                        Some(object)
                    } else {
                        None
                    }
                })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => result,
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Error while getting object info: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", err),
            ))
        }
    };

    let object = match object {
        Some(object) => object,
        None => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not get object info for object_id {object_id}."),
            ))
        }
    };
    println!("{}", object);

    // TODO: add a way to print full object info i.e. raw bytes?
    // if *deep {
    //     println!("Full Info: {:#?}", object);
    // }

    Ok(HttpResponseOk(ObjectInfoResponse {
        owner: format!("{:?}", object.owner),
        version: format!("{:?}", object.version().value()),
        id: format!("{:?}", object.id()),
        readonly: format!("{:?}", object.is_read_only()),
        obj_type: format!(
            "{:?}",
            object
                .data
                .type_()
                .map_or("Type Unwrap Failed".to_owned(), |type_| type_
                    .module
                    .as_ident_str()
                    .to_string())
        ),
    }))
}

/**
* 'SyncRequest' represents the sync request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct SyncRequest {
    address: String,
}

/**
 * [WALLET] Synchronize client state with authorities.
 */
#[endpoint {
    method = PATCH,
    path = "/wallet/sync",
}]
async fn sync(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();
    let sync_params = request.into_inner();
    let address = match decode_address_hex(sync_params.address.as_str()) {
        Ok(address) => address,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
            ))
        }
    };

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();

    let client_state = match wallet_context.get_or_create_client_state(&address) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    address
                ),
            ))
        }
    };

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
                Ok(_) => (),
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Sync error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Sync error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Sync error: {:?}", err),
            ))
        }
    };

    Ok(HttpResponseUpdatedNoContent())
}

/**
* 'TransferOrderRequest' represents the transaction to be sent to the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct TransferOrderRequest {
    object_id: String,
    to_address: String,
    gas_object_id: String,
}

/**
* 'OrderResponse' represents the call response
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct OrderResponse {
    object_effects_summary: Vec<String>,
    certificate: String,
}

/**
 * [WALLET] Transfer object.
 */
#[endpoint {
    method = PATCH,
    path = "/wallet/transfer",
}]
async fn transfer_object(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<TransferOrderRequest>,
) -> Result<HttpResponseOk<OrderResponse>, HttpError> {
    let server_context = rqctx.context();

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();

    let transfer_order_params = request.into_inner();

    let to_address = match decode_address_hex(transfer_order_params.to_address.as_str()) {
        Ok(to_address) => to_address,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode to address from hex {error}"),
            ))
        }
    };
    let object_id = match AccountAddress::try_from(transfer_order_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };
    let gas_object_id = match AccountAddress::try_from(transfer_order_params.gas_object_id) {
        Ok(gas_object_id) => gas_object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    let owner = match wallet_context.find_owner(&gas_object_id) {
        Ok(owner) => owner,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{err}"),
            ))
        }
    };

    let client_state = match wallet_context.get_or_create_client_state(&owner) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    owner
                ),
            ))
        }
    };

    let (cert, effects) = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // transfer object
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    client_state
                        .transfer_object(object_id, gas_object_id, to_address)
                        .await
                })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => match result {
                Ok((cert, effects)) => {
                    if effects.status != ExecutionStatus::Success {
                        return Err(HttpError::for_client_error(
                            None,
                            hyper::StatusCode::FAILED_DEPENDENCY,
                            format!("Error publishing module: {:#?}", effects.status),
                        ));
                    }
                    (cert, effects)
                }
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Publishing error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Publishing error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Publishing error: {:?}", err),
            ))
        }
    };

    let certificate = format!("{:?}", cert).to_string();
    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate,
    }))
}

/**
* 'PublishRequest' represents the publish request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct PublishRequest {
    path: String,
    gas_object_id: String,
}

/**
 * [WALLET] Publish move module.
 */
#[endpoint {
    method = PATCH,
    path = "/wallet/publish",
}]
async fn publish(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<PublishRequest>,
) -> Result<HttpResponseOk<OrderResponse>, HttpError> {
    let server_context = rqctx.context();
    let publish_params = request.into_inner();

    // TODO: figure out what a move module path looks like? Convert bytes string to moudle?
    let path = publish_params.path;

    let gas_object_id = match AccountAddress::try_from(publish_params.gas_object_id) {
        Ok(gas_object_id) => gas_object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();

    let owner = match wallet_context.find_owner(&gas_object_id) {
        Ok(account) => account,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    let client_state = match wallet_context.get_or_create_client_state(&owner) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    owner
                ),
            ))
        }
    };

    let gas_obj_ref = *client_state
        .object_refs()
        .get(&gas_object_id)
        .expect("Gas object not found");

    let (cert, effects) = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // publish
                let rt = Runtime::new().unwrap();
                rt.block_on(async move { client_state.publish(path, gas_obj_ref).await })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => match result {
                Ok((cert, effects)) => {
                    if effects.status != ExecutionStatus::Success {
                        return Err(HttpError::for_client_error(
                            None,
                            hyper::StatusCode::FAILED_DEPENDENCY,
                            format!("Error publishing module: {:#?}", effects.status),
                        ));
                    }
                    (cert, effects)
                }
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Move call error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Move call error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Move call error: {:?}", err),
            ))
        }
    };

    let certificate = format!("{:?}", cert).to_string();
    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate,
    }))
}

/**
* 'CallRequest' represents the call request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct CallRequest {
    package_object_id: String,
    module: String,
    function: String,
    type_args: Vec<String>,
    object_args: Vec<String>,
    pure_args: Vec<String>,
    gas_object_id: String,
    gas_budget: u64,
}

/**
 * [WALLET] Call move module.
 */
#[endpoint {
    method = PATCH,
    path = "/wallet/call",
}]
async fn call(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<CallRequest>,
) -> Result<HttpResponseOk<OrderResponse>, HttpError> {
    let server_context = rqctx.context();
    let call_params = request.into_inner();

    let module = call_params.module.to_owned();
    let function = call_params.function.to_owned();

    let mut pure_args = Vec::new();
    let pure_args_strings = call_params.pure_args;
    for pure_args_string in pure_args_strings {
        println!("{}", pure_args_string);
        pure_args.push(parse_transaction_argument(&pure_args_string).unwrap());
    }

    let mut type_args = Vec::new();
    let type_args_strings = call_params.type_args;
    for type_args_string in type_args_strings {
        type_args.push(parse_type_tag(&type_args_string).unwrap());
    }

    let gas_object_id = match AccountAddress::try_from(call_params.gas_object_id) {
        Ok(gas_object_id) => gas_object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Gas Object ID: {error}"),
            ))
        }
    };

    println!("{}", call_params.package_object_id);

    let package_object_id = match AccountAddress::from_hex_literal(&call_params.package_object_id) {
        Ok(package_object_id) => package_object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Package Object ID: {error}"),
            ))
        }
    };

    let mut object_args = Vec::new();
    let object_args_strings = call_params.object_args;
    for object_args_string in object_args_strings {
        let object_arg = match AccountAddress::try_from(object_args_string) {
            Ok(gas_object_id) => gas_object_id,
            Err(error) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Object Args: {error}"),
                ))
            }
        };
        object_args.push(object_arg);
    }

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    if wallet_context.is_none() {
        return Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist. Resync wallet via endpoint /wallet/addresses."
                .to_string(),
        ));
    }
    let wallet_context = wallet_context.as_mut().unwrap();

    // Find owner of gas object
    let sender = match wallet_context.find_owner(&gas_object_id) {
        Ok(account) => account,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not find owner for gas object: {error}"),
            ))
        }
    };

    let client_state = match wallet_context.get_or_create_client_state(&sender) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    sender
                ),
            ))
        }
    };

    let package_obj_info = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Get the object info
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Ok(ObjectRead::Exists(object_refs, object)) =
                        client_state.get_object_info(package_object_id).await
                    {
                        Some(ObjectRead::Exists(object_refs, object))
                    } else {
                        None
                    }
                })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => result,
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Error while getting object info: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", err),
            ))
        }
    };

    let package_obj_info = match package_obj_info {
        Some(object) => object,
        None => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not get object info for object_id {package_object_id}."),
            ))
        }
    };
    println!("{:?}", package_obj_info.object());
    let package_obj_ref = package_obj_info.object().unwrap().to_object_reference();

    let client_state = match wallet_context.get_or_create_client_state(&sender) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    sender
                ),
            ))
        }
    };

    // Fetch the object info for the gas obj
    let gas_obj_ref = *client_state
        .object_refs()
        .get(&gas_object_id)
        .expect("Gas object not found");

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in object_args {
        let client_state = match wallet_context.get_or_create_client_state(&sender) {
            Ok(client_state) => client_state,
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!(
                        "Could not create or get client state for {:?}: {err}",
                        sender
                    ),
                ))
            }
        };

        let obj_info = match cb_thread::scope(|scope| {
            scope
                .spawn(|_| {
                    // Get the object info
                    let rt = Runtime::new().unwrap();
                    rt.block_on(async move {
                        if let Ok(ObjectRead::Exists(object_refs, object)) =
                            client_state.get_object_info(obj_id).await
                        {
                            Some(ObjectRead::Exists(object_refs, object))
                        } else {
                            None
                        }
                    })
                })
                .join()
        }) {
            Ok(result) => match result {
                Ok(result) => result,
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Error while getting object info: {:?}", err),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Error while getting object info: {:?}", err),
                ))
            }
        };

        let obj_info = match obj_info {
            Some(object) => object,
            None => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Could not get object info for object_id {obj_id}."),
                ))
            }
        };

        let obj = match obj_info.object() {
            Ok(obj) => obj,
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Could not get object info for object_id {obj_id}: {err}"),
                ))
            }
        };

        object_args_refs.push(obj.to_object_reference());
    }

    let client_state = match wallet_context.get_or_create_client_state(&sender) {
        Ok(client_state) => client_state,
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not create or get client state for {:?}: {err}",
                    sender
                ),
            ))
        }
    };

    let (cert, effects) = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // execute move call
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    client_state
                        .move_call(
                            package_obj_ref,
                            Identifier::from_str(&module).unwrap(),
                            Identifier::from_str(&function).unwrap(),
                            type_args.clone(),
                            gas_obj_ref,
                            object_args_refs,
                            convert_txn_args(&pure_args),
                            call_params.gas_budget,
                        )
                        .await
                })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => match result {
                Ok((cert, effects)) => (cert, effects),
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Move call error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Move call error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Move call error: {:?}", err),
            ))
        }
    };

    let certificate = format!("{:?}", cert).to_string();
    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate,
    }))
}

fn get_object_effects(order_effects: OrderEffects) -> Vec<String> {
    let mut object_effects_summary = Vec::new();
    if !order_effects.created.is_empty() {
        println!("Created Objects:");
        for (obj, _) in order_effects.created {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2).to_string());
        }
    }
    if !order_effects.mutated.is_empty() {
        println!("Mutated Objects:");
        for (obj, _) in order_effects.mutated {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2).to_string());
        }
    }
    if !order_effects.deleted.is_empty() {
        println!("Deleted Objects:");
        for obj in order_effects.deleted {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2));
        }
    }
    object_effects_summary
}

async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    pre_load_objects: &[FastxObject],
    buffer_size: usize,
) -> AuthorityServer {
    let path = PathBuf::from(authority.db_path.clone());
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
