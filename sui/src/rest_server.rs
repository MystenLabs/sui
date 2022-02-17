// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crossbeam::thread as cb_thread;

use dropshot::{endpoint, Query};
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
use sui_types::committee::Committee;
use sui_types::messages::{ExecutionStatus, OrderEffects};
use sui_types::object::Object as SuiObject;
use sui_types::{base_types::*, object::ObjectRead};

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
        .to_logger("rest-api")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    tracing_subscriber::fmt().init();

    let mut api = ApiDescription::new();
    api.register(start).unwrap();
    api.register(genesis).unwrap();
    api.register(stop).unwrap();
    api.register(get_addresses).unwrap();
    api.register(get_objects).unwrap();
    api.register(object_info).unwrap();
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

        if let Some(err) = sync_client_state(client_state) {
            return Err(err);
        }
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
    object_ref: serde_json::Value,
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
    query: Query<GetObjectsRequest>,
) -> Result<HttpResponseOk<GetObjectsResponse>, HttpError> {
    let server_context = rqctx.context();

    let get_objects_params = query.into_inner();
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
                object_ref: json!(e.1),
            })
            .collect::<Vec<Object>>(),
    }))
}

/**
* `GetObjectInfoRequest` represents the request to get object info.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectInfoRequest {
    owner: String,
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
    data: serde_json::Value,
}

/**
 * [WALLET] Get object info.
 */
#[endpoint {
    method = GET,
    path = "/wallet/object_info",
}]
async fn object_info(
    rqctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    let server_context = rqctx.context();
    let object_info_params = query.into_inner();

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
    let object_id = match AccountAddress::try_from(object_info_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ))
        }
    };

    let owner = match decode_address_hex(object_info_params.owner.as_str()) {
        Ok(owner) => owner,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
            ))
        }
    };

    // Fetch the object ref
    let client_state = match wallet_context.get_or_create_client_state(&owner) {
        Ok(client_state) => client_state,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not get client state for account {:?}: {error}",
                    owner
                ),
            ))
        }
    };

    let object = match get_object_info(client_state, object_id) {
        Ok(ObjectRead::Exists(_, object, _)) => object,
        Ok(ObjectRead::Deleted(_)) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) was deleted."),
            ))
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) does not exist."),
            ))
        }
        Err(err) => return Err(err),
    };

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
        data: json!(object),
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
    method = POST,
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

    if let Some(err) = sync_client_state(client_state) {
        return Err(err);
    }

    Ok(HttpResponseUpdatedNoContent())
}

/**
* 'TransferOrderRequest' represents the transaction to be sent to the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct TransferOrderRequest {
    from_address: String,
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
    certificate: serde_json::Value,
}

/**
 * [WALLET] Transfer object.
 */
#[endpoint {
    method = POST,
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

    let owner = match decode_address_hex(transfer_order_params.from_address.as_str()) {
        Ok(owner) => owner,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
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
                    if !matches!(effects.status, ExecutionStatus::Success { .. }) {
                        return Err(HttpError::for_client_error(
                            None,
                            hyper::StatusCode::FAILED_DEPENDENCY,
                            format!("Error transferring object: {:#?}", effects.status),
                        ));
                    }
                    (cert, effects)
                }
                Err(err) => {
                    return Err(HttpError::for_client_error(
                        None,
                        hyper::StatusCode::FAILED_DEPENDENCY,
                        format!("Transfer error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Transfer error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Transfer error: {:?}", err),
            ))
        }
    };

    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate: json!(cert),
    }))
}

/**
* 'PublishRequest' represents the publish request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct PublishRequest {
    sender: String,
    path: String,
    gas_object_id: String,
    gas_budget: u64,
}

/**
 * [WALLET] Publish move module.
 */
#[endpoint {
    method = POST,
    path = "/wallet/publish",
}]
async fn publish(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<PublishRequest>,
) -> Result<HttpResponseOk<OrderResponse>, HttpError> {
    let server_context = rqctx.context();
    let publish_params = request.into_inner();

    // TODO: figure out what a move module path looks like? Convert bytes string to module?
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

    let sender = match decode_address_hex(publish_params.sender.as_str()) {
        Ok(sender) => sender,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
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

    // Fetch the object info for the gas obj
    let gas_obj_ref = match (*client_state).object_refs().get(&gas_object_id) {
        Some(gas_obj_ref) => *gas_obj_ref,
        None => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                "Gas object (gas_object_id) not found".to_string(),
            ))
        }
    };

    let (cert, effects) = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // publish
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    client_state
                        .publish(path, gas_obj_ref, publish_params.gas_budget)
                        .await
                })
            })
            .join()
    }) {
        Ok(result) => match result {
            Ok(result) => match result {
                Ok((cert, effects)) => {
                    if !matches!(effects.status, ExecutionStatus::Success { .. }) {
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
                        format!("Publish error: {err}"),
                    ))
                }
            },
            Err(err) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Publish error: {:?}", err),
                ))
            }
        },
        Err(err) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Publish error: {:?}", err),
            ))
        }
    };

    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate: json!(cert),
    }))
}

/**
* 'CallRequest' represents the call request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct CallRequest {
    sender: String,
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
    method = POST,
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
            Ok(object_arg) => object_arg,
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

    let sender = match decode_address_hex(call_params.sender.as_str()) {
        Ok(sender) => sender,
        Err(error) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
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

    let package_obj_ref = match get_object_info(client_state, package_object_id) {
        Ok(ObjectRead::Exists(object_ref, _, _)) => object_ref,
        Ok(ObjectRead::Deleted(_)) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Object ({package_object_id}) was deleted."),
            ))
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                format!("Object ({package_object_id}) does not exist."),
            ))
        }
        Err(err) => return Err(err),
    };

    // Fetch the object info for the gas obj
    let gas_obj_ref = match (*client_state).object_refs().get(&gas_object_id) {
        Some(gas_obj_ref) => *gas_obj_ref,
        None => {
            return Err(HttpError::for_client_error(
                None,
                hyper::StatusCode::FAILED_DEPENDENCY,
                "Gas object (gas_object_id) not found".to_string(),
            ))
        }
    };

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in object_args {
        let obj = match get_object_info(client_state, obj_id) {
            Ok(ObjectRead::Exists(_, obj, _)) => obj,
            Ok(ObjectRead::Deleted(_)) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Object ({obj_id}) was deleted."),
                ))
            }
            Ok(ObjectRead::NotExists(_)) => {
                return Err(HttpError::for_client_error(
                    None,
                    hyper::StatusCode::FAILED_DEPENDENCY,
                    format!("Object ({obj_id}) does not exist."),
                ))
            }
            Err(err) => return Err(err),
        };

        object_args_refs.push(obj.to_object_reference());
    }

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

    let object_effects_summary = get_object_effects(effects);

    Ok(HttpResponseOk(OrderResponse {
        object_effects_summary,
        certificate: json!(cert),
    }))
}

fn get_object_effects(order_effects: OrderEffects) -> Vec<String> {
    let mut object_effects_summary = Vec::new();
    if !order_effects.created.is_empty() {
        object_effects_summary.push(String::from("Created Objects:"));
        for (obj, _) in order_effects.created {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2).to_string());
        }
    }
    if !order_effects.mutated.is_empty() {
        object_effects_summary.push(String::from("Mutated Objects:"));
        for (obj, _) in order_effects.mutated {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2).to_string());
        }
    }
    if !order_effects.deleted.is_empty() {
        object_effects_summary.push(String::from("Deleted Objects:"));
        for obj in order_effects.deleted {
            object_effects_summary.push(format!("{:?} {:?} {:?}", obj.0, obj.1, obj.2));
        }
    }
    object_effects_summary
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

fn get_object_info(
    client_state: &mut ClientState<AuthorityClient>,
    object_id: AccountAddress,
) -> Result<ObjectRead, HttpError> {
    let obj_info = match cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Get the object info
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Ok(ObjectRead::Exists(object_refs, object, move_struct_layout)) =
                        client_state.get_object_info(object_id).await
                    {
                        Some(ObjectRead::Exists(object_refs, object, move_struct_layout))
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

    match obj_info {
        Some(object) => Ok(object),
        None => Err(HttpError::for_client_error(
            None,
            hyper::StatusCode::FAILED_DEPENDENCY,
            format!("Could not get object info for object_id: {object_id}."),
        )),
    }
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
