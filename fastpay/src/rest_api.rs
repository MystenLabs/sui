// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

mod client;
mod server;

use crossbeam::thread as cb_thread;

use dropshot::endpoint;
use dropshot::ApiDescription;
use dropshot::ConfigDropshot;
use dropshot::ConfigLogging;
use dropshot::ConfigLoggingLevel;
use dropshot::HttpError;
use dropshot::HttpResponseOk;
use dropshot::HttpResponseUpdatedNoContent;
use dropshot::HttpServerStarter;
use dropshot::RequestContext;
use dropshot::TypedBody;

use fastpay::config::{AccountsConfig, CommitteeConfig};
use fastx_network::transport;
use fastx_types::base_types::*;

use move_core_types::account_address::AccountAddress;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use tempfile::tempdir;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::path::PathBuf;

use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

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
    // Use mpsc channels to send terminating message and kill thread
    // api.register(stop).unwrap();
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
            accounts_config: Arc::new(Mutex::new(
                AccountsConfig::read_or_create(accounts_config_path.as_str()).unwrap(),
            )),
            committee_config: Arc::new(Mutex::new(
                CommitteeConfig::read(committee_config_path.as_str()).unwrap(),
            )),
        }
    }
}


// /**
//  * [SERVER] Use to provide server configurations for genesis.
//  */
// #[endpoint {
//     method = POST,
//     path = "/fastx/genesis",
// }]
// async fn genesis(
//     rqctx: Arc<RequestContext<ServerContext>>,
//     configuration: TypedBody<ServerConfiguration>,
// ) -> Result<HttpResponseUpdatedNoContent, HttpError> {
//     let mut config =
//         NetworkConfig::read_or_create(&network_conf_path).expect("Unable to read user accounts");

//     if !config.authorities.is_empty() {
//         println!("Cannot run genesis on a existing network, please delete network config file and try again.");
//         exit(1);
//     }

//     let mut authorities = BTreeMap::new();
//     let mut authority_info = Vec::new();
//     let mut port_allocator = PortAllocator::new(10000);

//     println!("Creating new addresses...");
//     for _ in 0..4 {
//         let (address, key_pair) = get_key_pair();
//         let info = AuthorityPrivateInfo {
//             address,
//             key_pair,
//             host: "127.0.0.1".to_string(),
//             port: port_allocator.next_port().expect("No free ports"),
//             db_path: format!("./authorities_db/{:?}", address),
//         };
//         authority_info.push(AuthorityInfo {
//             address,
//             host: info.host.clone(),
//             base_port: info.port,
//         });
//         authorities.insert(info.address, 1);
//         config.authorities.push(info);
//     }

//     config.save()?;

//     let mut new_addresses = Vec::new();
//     let mut preload_objects = Vec::new();

//     println!("Creating test objects...");
//     for _ in 0..5 {
//         let (address, key_pair) = get_key_pair();
//         new_addresses.push(AccountInfo { address, key_pair });
//         for _ in 0..5 {
//             let new_object = Object::with_id_owner_gas_coin_object_for_testing(
//                 ObjectID::random(),
//                 SequenceNumber::new(),
//                 address,
//                 1000,
//             );
//             preload_objects.push(new_object);
//         }
//     }
//     let committee = Committee::new(authorities);

//     // Make server state to persist the objects.
//     for authority in config.authorities {
//         make_server(&authority, &committee, &preload_objects, config.buffer_size).await;
//     }

//     let wallet_config = WalletConfig {
//         accounts: new_addresses,
//         authorities: authority_info,
//         send_timeout: Duration::from_micros(4000000),
//         recv_timeout: Duration::from_micros(4000000),
//         buffer_size: config.buffer_size,
//         db_folder_path: "./client_db".to_string(),
//         config_path: "./wallet.conf".to_string(),
//     };
//     wallet_config.save()?;

//     println!("Network genesis completed.");
//     println!("Network config file is stored in {}.", config.config_path);
//     println!(
//         "Wallet config file is stored in {}.",
//         wallet_config.config_path
//     );
// }



/**ee
* [INPUT] `Server Configuration` represents the provided server configuration.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct ServerConfiguration {
    num_servers: u32,
    // Make optional and provide defaults?
    send_timeout_secs: u64,
    recv_timeout_secs: u64,
}

/**
 * [SERVER] Start servers with specified configurations.
 */
#[endpoint {
    method = POST,
    path = "/fastx/start",
}]
async fn start(
    rqctx: Arc<RequestContext<ServerContext>>,
    configuration: TypedBody<ServerConfiguration>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();
    let configuration = configuration.into_inner();

    let num_servers = configuration.num_servers;

    *server_context.send_timeout.lock().unwrap() =
        Duration::from_secs(configuration.send_timeout_secs);
    *server_context.recv_timeout.lock().unwrap() =
        Duration::from_secs(configuration.recv_timeout_secs);

    let accounts_config_path = server_context
        .accounts_config_path
        .lock()
        .unwrap()
        .to_owned();
    let committee_config_path = server_context
        .committee_config_path
        .lock()
        .unwrap()
        .to_owned();
    let initial_accounts_config_path = server_context
        .initial_accounts_config_path
        .lock()
        .unwrap()
        .to_owned();

    let mut accounts_config = AccountsConfig::read_or_create(&accounts_config_path).unwrap();

    // Generate configs for the servers (could split this out to its own endpoint)
    for i in 0..num_servers {
        let db_dir = "db".to_string() + &i.to_string();
        let server_config_path = "server".to_string() + &i.to_string() + ".json";
        fs::create_dir(&db_dir)
            .unwrap_or_else(|_| panic!("Failed to create database directory: {}", db_dir));
        File::create(&server_config_path).expect("Couldn't create server config file");
        let server = server::create_server_config(
            server_config_path.as_str(),
            "0.0.0.0".to_string(),
            (9100 + i).try_into().unwrap(),
            db_dir,
        );

        // Write to committee_config_path
        let mut file = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&committee_config_path)
            .unwrap();
        file.write_all(serde_json::to_string(&server.authority).unwrap().as_bytes())
            .ok();
    }

    let committee_config = CommitteeConfig::read(&committee_config_path).unwrap();

    // Create accounts with starting values (could split this out to its own endpoint)
    let _initial_state_config = client::create_account_configs(
        100,
        10,
        2000000,
        &accounts_config_path,
        &mut accounts_config,
        &initial_accounts_config_path,
    );

    *server_context.accounts_config.lock().unwrap() = accounts_config;
    *server_context.committee_config.lock().unwrap() = committee_config;

    let buffer_size: usize = server_context.buffer_size;

    let mut thrs = Vec::new();

    for i in 0..num_servers {
        let server_config_path = "server".to_string() + &i.to_string() + ".json";
        let committee_config_path = committee_config_path.clone();
        let initial_accounts_config_path = initial_accounts_config_path.clone();

        thrs.push(thread::spawn(move || {
            println!("Starting...");

            server::run_server(
                &server_config_path,
                &committee_config_path,
                &initial_accounts_config_path,
                buffer_size,
            );
        }));
    }

    Ok(HttpResponseUpdatedNoContent())
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
    path = "/server/addresses",
}]
async fn get_addresses(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<Addresses>, HttpError> {
    let server_context = rqctx.context();

    let accounts_config = &mut *server_context.accounts_config.lock().unwrap();

    let addresses = accounts_config
        .addresses()
        .into_iter()
        .map(|addr| format!("{:X}", addr).trim_end_matches('=').to_string())
        .collect();

    Ok(HttpResponseOk(Addresses { addresses }))
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
