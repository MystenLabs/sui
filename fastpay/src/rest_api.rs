// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

// REMOVE THIS
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

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

use fastpay::{
    config::{AccountsConfig, AuthorityServerConfig, CommitteeConfig, InitialStateConfig},
};
use fastx_network::transport;
use fastx_types::{
    base_types::*,
    messages::Order,
};

use http::StatusCode;
use hyper::Body;
use hyper::Response;

use move_core_types::{account_address::AccountAddress, transaction_argument::convert_txn_args};

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use threadpool::ThreadPool;

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  Requesting
     * a specific port so we can use ngrok to expose that port publicly.
     */
    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((Ipv6Addr::LOCALHOST, 5000)),
        ..Default::default()
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("example-basic")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    /*
     * Build a description of the API.
     */
    let mut api = ApiDescription::new();
    // Store threads in a vector of custom thread structs
    // struct threadholder { handle, rx, tx }
    api.register(start).unwrap();
    // Use mpsc channels to send terminating message and kill thread
    // api.register(stop).unwrap();
    api.register(get_accounts).unwrap();
    api.register(get_account_objects).unwrap();
    api.register(get_object_info).unwrap();
    api.register(transfer_object).unwrap();

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let api_context = ServerContext::new();

    /*
     * Set up the server.
     */
    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    /*
     * Wait for the server to stop.  Note that there's not any code to shut down
     * this server, so we should never get past this point.
     */
    server.await
}

// struct ThreadContainer {
//     tx: Mutex<Sender<String>>,
//     // rx: Receiver<String>,
//     handle: JoinHandle<String>,
// }

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    /** Server configuration that can be manipulated by requests to the HTTP API */

    // also should store the threads with servers to check status
    threadpool: Arc<Mutex<ThreadPool>>,
    initial_state_cfg: Arc<Mutex<InitialStateConfig>>,
    buffer_size: usize,
    send_timeout: Arc<Mutex<Duration>>,
    recv_timeout: Arc<Mutex<Duration>>,
    acc_cfg: Arc<Mutex<AccountsConfig>>,
    committee_cfg: Arc<Mutex<CommitteeConfig>>,


    // Do I need any of the below???
    num_servers: AtomicU64,
    auth_serv_cfgs: Vec<AuthorityServerConfig>,
}

impl ServerContext {
    /**
     * Return a new ServerContext.
     */
    pub fn new() -> ServerContext {
        ServerContext {
            //TODO: How to default this to nothing? 
            threadpool: Arc::new(Mutex::new(ThreadPool::new(1))),
            initial_state_cfg: Arc::new(Mutex::new(InitialStateConfig::new())),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse().unwrap(),
            send_timeout: Arc::new(Mutex::new(Duration::new(0, 0))),
            recv_timeout: Arc::new(Mutex::new(Duration::new(0, 0))),
            acc_cfg: Arc::new(Mutex::new(AccountsConfig::read_or_create("").unwrap())),
            committee_cfg: Arc::new(Mutex::new(CommitteeConfig::read("").unwrap())),


            num_servers: AtomicU64::new(0),
            auth_serv_cfgs: Vec::new()
        }
    }
}

/*
 * HTTP API interface
 */

/**
 * `Server Status` represents the current status of the server with the most recently
 * provided server configuration.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct ServerStatus {
    status: Vec<String>,
}

/**
* `Server Configuration` represents the provided server configuration.
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
    path = "/server/start",
}]
async fn start(
    rqctx: Arc<RequestContext<ServerContext>>,
    configuration: TypedBody<ServerConfiguration>,
) -> Result<HttpResponseOk<ServerStatus>, HttpError> {
    let server_context = rqctx.context();
    let configuration = configuration.into_inner();

    let num_servers = configuration.num_servers;

    *server_context.send_timeout.lock().unwrap() = Duration::from_secs(configuration.send_timeout_secs);
    *server_context.recv_timeout.lock().unwrap() = Duration::from_secs(configuration.recv_timeout_secs);

    // Create some account config container
    let mut acc_cfg = AccountsConfig::read_or_create("").unwrap();
    // Committee config is an aggregate of auth configs
    let mut committee_cfg = CommitteeConfig::read("").unwrap();

    // Generate configs for the servers
    let mut auth_serv_cfgs = Vec::new();
    for i in 0..num_servers {
        let db_dir = "db".to_owned() + &i.to_string();
        let s = server::create_server_config(
            "127.0.0.1".to_string(),
            (9100 + i).try_into().unwrap(),
            db_dir,
        );
        let auth = AuthorityServerConfig {
            authority: s.authority.clone(),
            key: s.key.copy(),
        };
        auth_serv_cfgs.push(auth);

        committee_cfg.authorities.push(s.authority.clone());
    }

    // Create accounts with starting values
    let initial_state_cfg = client::create_account_configs(
        5,
        3,
        &mut acc_cfg,
    );

    *server_context.acc_cfg.lock().unwrap() = acc_cfg;
    *server_context.committee_cfg.lock().unwrap() = committee_cfg.clone();
    *server_context.initial_state_cfg.lock().unwrap() = initial_state_cfg.clone();

    let buffer_size: usize = server_context.buffer_size;

    // Thread handles for servers
    let mut thrs = Vec::new();

    let mut status = Vec::new();

    // Run the servers with the inite values
    for i in 0..num_servers {
        let s = auth_serv_cfgs.get(i as usize).unwrap();
        let auth = AuthorityServerConfig {
            authority: s.authority.clone(),
            key:s.key.copy(),
        };

        let cfg = committee_cfg.clone();
        let init_cfg = initial_state_cfg.clone();

        let status_string = format!(
            "Server {:?} running on {}:{}", 
             s.authority.address, s.authority.host, s.authority.base_port
        );

        println!("{}", status_string );

        status.push(status_string);

        // let (tx, rx) = mpsc::channel();

        thrs.push(thread::spawn(move || {

            println!("Starting...");

            server::run_server(
                &"".to_string(),
                "".to_string(), 
                "".to_string(), 
                buffer_size);

            // println!("Working...");

            // loop {
            //     thread::sleep(Duration::from_millis(500));
            //     match rx.try_recv() {
            //         Ok(_) | Err(TryRecvError::Disconnected) => {
            //             println!("Terminating.");
            //             break;
            //         }
            //         Err(TryRecvError::Empty) => {}
            //     }
            // }
            // tx.send("Terminate".to_string());
            }
        ));
    }

    Ok(HttpResponseOk(ServerStatus { status }))
}

/**
 * `Accounts` represents the value of the accounts on the network.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Accounts {
    accounts: Vec<String>,
}

/**
 * [SERVER] Retrieve all accounts (addresses) setup by initial configuration. 
 */
#[endpoint {
    method = GET,
    path = "/server/accounts",
}]
async fn get_accounts(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<Accounts>, HttpError> {
    let server_context = rqctx.context();

    // TODO: Error handle here instead of unwrap()
    let init_cfg = server_context.initial_state_cfg.lock().unwrap();

    let accounts = init_cfg.config
        .iter()
        .map(|entry| format!("{:?}", entry.address))
        .collect();

    Ok(HttpResponseOk(Accounts { accounts }))
}



/**
* `Account` represents the provided account.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct Account {
    // Use accountaddress but it doesn't implement JsonSchema
    account_address: String,
}


/**
 * `Object` represents the value of the objects on the network.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Object {
    object_id: String,
    object_ref: String,
}

/**
 * `Objects` is a collection of Object
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct Objects {
    objects: Vec<Object>,
}

/**
 * [CLIENT] Return all objects for a specified account. 
 */
#[endpoint {
    method = GET,
    path = "/client/account_objects",
}]
async fn get_account_objects(
    rqctx: Arc<RequestContext<ServerContext>>,
    account: TypedBody<Account>,
) -> Result<HttpResponseOk<Objects>, HttpError> {

    let server_context = rqctx.context();

    let send_timeout = *server_context.send_timeout.lock().unwrap();
    let recv_timeout = *server_context.recv_timeout.lock().unwrap();
    let buffer_size = server_context.buffer_size.clone();
    let mut account_config = &mut *server_context.acc_cfg.lock().unwrap();
    let committee_config = &*server_context.committee_cfg.lock().unwrap();

    let acc_objs = cb_thread::scope(|scope| {
        scope.spawn(|_| {
            // Get the objects for account
            client::query_objects(
                &mut account_config,
                &committee_config,
                decode_address_hex(account.into_inner().account_address.as_str()).unwrap(),
                buffer_size,
                send_timeout,
                recv_timeout,
                
            )
        }).join().unwrap()
    }).unwrap();

    Ok(HttpResponseOk(Objects{ objects: 
        acc_objs
            .into_iter()
            .map(|e| Object{ object_id: e.1.0.to_string(), object_ref: format!("{:?}", e.1) })
            .collect::<Vec<Object>>()
    }))
}


/**
* `ObjectInfo` represents the object info on the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct ObjectInfo {
   owner: String,
   version: String,
   id: String,
   readonly: String,
   obj_type: String
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
    let buffer_size = server_context.buffer_size.clone();
    let mut account_config = &mut *server_context.acc_cfg.lock().unwrap();
    let committee_config = &*server_context.committee_cfg.lock().unwrap();

    let obj_info = cb_thread::scope(|scope| {
        scope.spawn(|_| {
            // Get the object info
            client::get_object_info(
                &mut account_config,
                &committee_config,
                AccountAddress::try_from(object.into_inner().object_id).unwrap(),
                buffer_size,
                send_timeout,
                recv_timeout,
            )
        }).join().unwrap()
    }).unwrap();

    Ok(HttpResponseOk(ObjectInfo{ 
        owner: format!("Owner: {:#?}", obj_info.object.owner), 
        version: format!("Version: {:#?}", obj_info.object.version().value()),
        id: format!("ID: {:#?}", obj_info.object.id()),
        readonly: format!("Readonly: {:#?}", obj_info.object.is_read_only()),
        obj_type: format!(
            "Type: {:#?}",
            obj_info
                .object
                .data
                .type_()
                .map_or("Type Unwrap Failed".to_owned(), |type_| type_
                    .module
                    .as_ident_str()
                    .to_string())
        )
    }))
}

/**
* [Input] `TransferOrder` represents the transaction to be sent to the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct TransferOrder {
    object_address: String,
    from_account: String,
    to_account: String,
    gas_address: String   
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
    let buffer_size = server_context.buffer_size.clone();
    let mut account_config = &mut *server_context.acc_cfg.lock().unwrap();
    let committee_config = &*server_context.committee_cfg.lock().unwrap();

    let from_account = decode_address_hex(transfer_order.from_account.as_str()).unwrap();
    let to_account = decode_address_hex(transfer_order.to_account.as_str()).unwrap();

    let object_id = AccountAddress::try_from(transfer_order.object_address).unwrap();
    let gas_object_id = AccountAddress::try_from(transfer_order.gas_address).unwrap();

    let acc_obj_info = cb_thread::scope(|scope| {
        scope.spawn(|_| {
            // Transfer from ACC1 to ACC2
            client::transfer_object(
                &mut account_config,
                &committee_config,
                object_id,
                gas_object_id,
                to_account,
                buffer_size,
                send_timeout,
                recv_timeout,
            )
        }).join().unwrap()
    }).unwrap();

    Ok(HttpResponseUpdatedNoContent())
} 