// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

mod client;
mod server;

use crossbeam::thread as cb_thread;

use dropshot::endpoint;
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, 
    HttpError, HttpResponseOk, HttpResponseUpdatedNoContent, HttpServerStarter,
    RequestContext, TypedBody
};

use fastpay::config::{
    AccountInfo, AccountsConfig, AuthorityInfo, AuthorityPrivateInfo, CommitteeConfig, 
    NetworkConfig, PortAllocator, WalletConfig
};
use fastpay::utils::Config;
use fastpay::wallet_commands::{WalletContext, WalletCommands};
use fastpay_core::authority::{AuthorityState, AuthorityStore};
use fastpay_core::authority_server::AuthorityServer;
use fastpay_core::client::Client;
use fastx_network::transport;
use fastx_types::base_types::*;
use fastx_types::committee::Committee;
use fastx_types::object::Object as FastxObject;

use futures::future::join_all;
use move_core_types::account_address::AccountAddress;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;
use tokio::runtime::Runtime;
use tracing::error;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    // Use mpsc channels to send terminating message and kill thread?
    api.register(stop).unwrap();
    api.register(get_addresses).unwrap();
    api.register(get_address_objects).unwrap();
    api.register(get_object_info).unwrap();
    api.register(transfer_object).unwrap();

    let accounts_config_path = "accounts.json".to_string();
    let initial_accounts_config_path = "initial_accounts.toml".to_string();
    File::create(&initial_accounts_config_path)
        .expect("Couldn't create initial accounts config file");
    let committee_config_path = "committee.json".to_string();
    File::create(&committee_config_path).expect("Couldn't create committee config file");
    let client_db_path = tempdir().unwrap().into_path();
    let api_context = ServerContext::new(
        accounts_config_path,
        committee_config_path,
        initial_accounts_config_path,
        client_db_path,
    );

    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    server.await
}

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    buffer_size: usize,
    send_timeout: Arc<Mutex<Duration>>,
    recv_timeout: Arc<Mutex<Duration>>,
    initial_accounts_config_path: Arc<Mutex<String>>,
    accounts_config_path: Arc<Mutex<String>>,
    committee_config_path: Arc<Mutex<String>>,
    client_db_path: Arc<Mutex<PathBuf>>,
    wallet_config_path: Arc<Mutex<String>>,
    network_config_path: Arc<Mutex<String>>,
    accounts_config: Arc<Mutex<AccountsConfig>>,
    committee_config: Arc<Mutex<CommitteeConfig>>,
}

impl ServerContext {
    pub fn new(
        accounts_config_path: String,
        committee_config_path: String,
        initial_accounts_config_path: String,
        client_db_path: PathBuf,
    ) -> ServerContext {
        ServerContext {
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE
                .to_string()
                .parse()
                .unwrap(),
            send_timeout: Arc::new(Mutex::new(Duration::new(0, 0))),
            recv_timeout: Arc::new(Mutex::new(Duration::new(0, 0))),
            initial_accounts_config_path: Arc::new(Mutex::new(initial_accounts_config_path)),
            accounts_config_path: Arc::new(Mutex::new(accounts_config_path.to_owned())),
            committee_config_path: Arc::new(Mutex::new(committee_config_path.to_owned())),
            client_db_path: Arc::new(Mutex::new(client_db_path)),
            wallet_config_path: Arc::new(Mutex::new(String::from("./wallet.conf"))),
            network_config_path: Arc::new(Mutex::new(String::from("./network.conf"))),
            accounts_config: Arc::new(Mutex::new(
                AccountsConfig::read_or_create(accounts_config_path.as_str()).unwrap(),
            )),
            committee_config: Arc::new(Mutex::new(
                CommitteeConfig::read(committee_config_path.as_str()).unwrap(),
            )),
        }
    }
}

/**
* 'GenesisRequest' represents the server configuration.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GenesisRequest {
    num_authorities: u32,
    num_objects: u32,
}


/**
 * 'GenesisResponse' returns the genesis wallet & network config.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GenesisResponse {
    wallet_config: String,
    network_config: String,
}

/**
 * [SERVER] Use to provide server configurations for genesis.
 */
#[endpoint {
    method = POST,
    path = "/fastx/genesis",
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
    _genesis_request: TypedBody<GenesisRequest>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    // TODO: Move to server startup code and add to server context

    let network_conf_path = String::from("./network.conf");

    let mut network_config = match NetworkConfig::read_or_create(&network_conf_path) {
        Ok(network_config) => network_config,
        Err(error) => return Err(
            HttpError::for_client_error(
                None, 
                hyper::StatusCode::FAILED_DEPENDENCY, 
                format!("Unable to read user accounts: {error}"))),
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

    println!("Creating new addresses...");
    for _ in 0..4 {
        let (address, key_pair) = get_key_pair();
        let info = AuthorityPrivateInfo {
            address,
            key_pair,
            host: "127.0.0.1".to_string(),
            port: port_allocator.next_port().expect("No free ports"),
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

    network_config.save().map_err(|err| 
        HttpError::for_client_error(
            None, 
            hyper::StatusCode::FAILED_DEPENDENCY, 
            format!("Network config was unable to be saved: {err}"))
        ).ok();

    let mut new_addresses = Vec::new();
    let mut preload_objects: Vec<FastxObject> = Vec::new();

    println!("Creating test objects...");
    for _ in 0..5 {
        let (address, key_pair) = get_key_pair();
        new_addresses.push(AccountInfo { address, key_pair });
        for _ in 0..5 {
            let new_object = FastxObject::with_id_owner_gas_coin_object_for_testing(
                ObjectID::random(),
                SequenceNumber::new(),
                address,
                1000,
            );
            preload_objects.push(new_object);
        }
    }
    let committee = Committee::new(authorities);

    // Make server state to persist the objects.
    let network_config_path = network_config.config_path().to_string();
    for authority in network_config.authorities.iter() {
        make_server(&authority, &committee, &preload_objects, network_config.buffer_size).await;
    }

    let wallet_config = match WalletConfig::create("./wallet.conf") {
        Ok(wallet_config) => wallet_config,
        Err(error) => return Err(HttpError::for_client_error(
            None, 
            hyper::StatusCode::FAILED_DEPENDENCY, 
            format!("Wallet config was unable to be created: {error}")))
    };
    wallet_config.save().map_err(|err| 
        HttpError::for_client_error(
            None, 
            hyper::StatusCode::FAILED_DEPENDENCY, 
            format!("Wallet config was unable to be saved: {err}"))
        ).ok();

    println!("Network genesis completed.");
    println!("Network config file is stored in {}.", network_config_path);
    println!(
        "Wallet config file is stored in {}.",
        wallet_config.config_path()
    );

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
        network_config: network_config_string
    }))
}

/**
 * [SERVER] Stop servers and delete storage.
 */
#[endpoint {
    method = POST,
    path = "/fastx/stop",
}]
async fn stop(
    _rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    // TODO move to server context
    let network_conf_path = String::from("./network.conf");
    let wallet_conf_path = String::from("./wallet.conf");
    let authority_db_dir = String::from("./authorities_db");

    fs::remove_dir_all(authority_db_dir).ok();
    fs::remove_file(network_conf_path).ok();
    fs::remove_file(wallet_conf_path).ok();

    Ok(HttpResponseUpdatedNoContent())
}

/**
 * [SERVER] Start servers with specified configurations.
 */
#[endpoint {
    method = POST,
    path = "/fastx/start",
}]
async fn start(
    _rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    // TODO: Move to server startup code and add to server context
    let network_conf_path = String::from("./network.conf");

    let network_config = match NetworkConfig::read_or_create(&network_conf_path) {
        Ok(network_config) => network_config,
        Err(error) => return Err(
            HttpError::for_client_error(
                None, 
                hyper::StatusCode::FAILED_DEPENDENCY, 
                format!("Unable to read user accounts: {error}"))),
    };

    if network_config.authorities.is_empty() {
        return Err(
            HttpError::for_client_error(
                None, 
                hyper::StatusCode::FAILED_DEPENDENCY, 
                String::from("No authority configured for the network, please run genesis.")));
    }

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

    let num_authorities = handles.len();

    thread::spawn(move || {
        rt.block_on(join_all(handles));
    });

    Ok(HttpResponseOk(format!("Started {} authorities", num_authorities)))
}

/**
 * [OUTPUT] `Accounts` represents the value of the accounts on the network.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Addresses {
    addresses: Vec<String>,
}

/**
 * [SERVER] Retrieve all addresses setup by initial configuration.
 */
#[endpoint {
    method = GET,
    path = "/fastx/addresses",
}]
async fn get_addresses(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<Addresses>, HttpError> {
    let server_context = rqctx.context();
    let wallet_config_path = server_context.wallet_config_path.lock().unwrap().clone();

    let config =
        WalletConfig::read_or_create(&wallet_config_path).expect("Unable to read wallet config");
    let addresses = config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut context = WalletContext::new(config);

    // Sync all accounts on start up.
    for address in addresses.iter() {
        let client_state = context
            .get_or_create_client_state(address).map_err(|err| 
                HttpError::for_client_error(
                None, 
                hyper::StatusCode::FAILED_DEPENDENCY, 
                format!("Can't create client state: {err}"))).ok().unwrap();
        client_state.sync_client_state()
            .await.map_err(|err| 
                HttpError::for_client_error(
                None, 
                hyper::StatusCode::FAILED_DEPENDENCY, 
                format!("Sync failed: {err}"))).ok();
    }
    Ok(HttpResponseOk(Addresses { 
        addresses: addresses
                        .into_iter()
                        .map(|address| format!("{:?}", address).to_string())
                        .collect() 
    }))
}

/**
* [INPUT] `Address` represents the provided address.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct Address {
    address: String,
}

/**
 * [INPUT & OUTPUT] `Object` represents the value of the objects on the network.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Object {
    object_id: String,
    object_ref: Option<String>,
}

/**
 * [OUTPUT] `Objects` is a collection of Object
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Objects {
    objects: Vec<Object>,
}

/**
 * [CLIENT] Return all objects for a specified address.
 */
#[endpoint {
    method = GET,
    path = "/client/address_objects",
}]
async fn get_address_objects(
    rqctx: Arc<RequestContext<ServerContext>>,
    account: TypedBody<Address>,
) -> Result<HttpResponseOk<Objects>, HttpError> {
    let server_context = rqctx.context();

    let send_timeout = *server_context.send_timeout.lock().unwrap();
    let recv_timeout = *server_context.recv_timeout.lock().unwrap();
    let buffer_size = server_context.buffer_size;
    let accounts_config_path = &*server_context.accounts_config_path.lock().unwrap();
    let client_db_path = server_context.client_db_path.lock().unwrap().clone();
    let account_config = &mut *server_context.accounts_config.lock().unwrap();
    let committee_config = &*server_context.committee_config.lock().unwrap();

    let acc_objs = cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Get the objects for account
                client::query_objects(
                    client_db_path,
                    accounts_config_path,
                    account_config,
                    committee_config,
                    decode_address_hex(account.into_inner().address.as_str()).unwrap(),
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                )
            })
            .join()
            .unwrap()
    })
    .unwrap();

    Ok(HttpResponseOk(Objects {
        objects: acc_objs
            .into_iter()
            .map(|e| Object {
                object_id: e.1 .0.to_string(),
                object_ref: Some(format!("{:?}", e.1)),
            })
            .collect::<Vec<Object>>(),
    }))
}

/**
* [OUTPUT] `ObjectInfo` represents the object info on the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct ObjectInfo {
    owner: String,
    version: String,
    id: String,
    readonly: String,
    obj_type: String,
}

/**
 * [CLIENT] Return object info.
 */
#[endpoint {
    method = GET,
    path = "/client/object_info",
}]
async fn get_object_info(
    rqctx: Arc<RequestContext<ServerContext>>,
    object: TypedBody<Object>,
) -> Result<HttpResponseOk<ObjectInfo>, HttpError> {
    let server_context = rqctx.context();

    let send_timeout = *server_context.send_timeout.lock().unwrap();
    let recv_timeout = *server_context.recv_timeout.lock().unwrap();
    let buffer_size = server_context.buffer_size;
    let account_config = &mut *server_context.accounts_config.lock().unwrap();
    let committee_config = &*server_context.committee_config.lock().unwrap();
    let client_db_path = server_context.client_db_path.lock().unwrap().clone();

    let obj_info = cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Get the object info
                client::get_object_info(
                    client_db_path,
                    account_config,
                    committee_config,
                    AccountAddress::try_from(object.into_inner().object_id).unwrap(),
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                )
            })
            .join()
            .unwrap()
    })
    .unwrap();

    Ok(HttpResponseOk(ObjectInfo {
        owner: format!("Owner: {:#?}", obj_info.owner),
        version: format!("Version: {:#?}", obj_info.version().value()),
        id: format!("ID: {:#?}", obj_info.id()),
        readonly: format!("Readonly: {:#?}", obj_info.is_read_only()),
        obj_type: format!(
            "Type: {:#?}",
            obj_info
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
* [INPUT] `TransferOrder` represents the transaction to be sent to the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct TransferOrder {
    object_id: String,
    to_address: String,
    gas_object_id: String,
}

/**
 * [CLIENT] Transfer object.
 */
#[endpoint {
    method = PATCH,
    path = "/client/transfer",
}]
async fn transfer_object(
    rqctx: Arc<RequestContext<ServerContext>>,
    transfer_order_body: TypedBody<TransferOrder>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();
    let transfer_order = transfer_order_body.into_inner();

    let send_timeout = *server_context.send_timeout.lock().unwrap();
    let recv_timeout = *server_context.recv_timeout.lock().unwrap();
    let buffer_size = server_context.buffer_size;
    let account_config = &mut *server_context.accounts_config.lock().unwrap();
    let committee_config = &*server_context.committee_config.lock().unwrap();
    let client_db_path = server_context.client_db_path.lock().unwrap().clone();
    let accounts_config_path = &*server_context.accounts_config_path.lock().unwrap();

    let to_address = decode_address_hex(transfer_order.to_address.as_str()).unwrap();
    let object_id = AccountAddress::try_from(transfer_order.object_id).unwrap();
    let gas_object_id = AccountAddress::try_from(transfer_order.gas_object_id).unwrap();

    let _acc_obj_info = cb_thread::scope(|scope| {
        scope
            .spawn(|_| {
                // Transfer from ACC1 to ACC2
                client::transfer_object(
                    client_db_path,
                    accounts_config_path,
                    account_config,
                    committee_config,
                    object_id,
                    gas_object_id,
                    to_address,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                )
            })
            .join()
            .unwrap()
    })
    .unwrap();

    Ok(HttpResponseUpdatedNoContent())
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
