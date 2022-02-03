// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::config::*;
use fastpay_core::{authority::*, authority_server::AuthorityServer};
use fastx_network::transport;
use fastx_types::{base_types::*, committee::Committee};

use futures::future::join_all;
use std::path::Path;
use std::sync::Arc;
use structopt::StructOpt;
use tokio::runtime::Runtime;
use tracing::subscriber::set_global_default;
use tracing::*;
use tracing_subscriber::EnvFilter;

#[allow(clippy::too_many_arguments)]
fn make_server(
    local_ip_addr: &str,
    server_config_path: &str,
    committee_config_path: &str,
    initial_accounts_config_path: &str,
    buffer_size: usize,
) -> AuthorityServer {
    let server_config =
        AuthorityServerConfig::read(server_config_path).expect("Fail to read server config");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Fail to read committee config");
    let initial_accounts_config = InitialStateConfig::read(initial_accounts_config_path)
        .expect("Fail to read initial account config");

    let committee = Committee::new(committee_config.voting_rights());

    let store = Arc::new(AuthorityStore::open(
        Path::new(&server_config.authority.database_path),
        None,
    ));

    // Load initial states
    let rt = Runtime::new().unwrap();

    let state = rt.block_on(async {
        let state = AuthorityState::new_with_genesis_modules(
            committee,
            server_config.authority.address,
            server_config.key.copy(),
            store,
        )
        .await;
        for initial_state_cfg_entry in initial_accounts_config.config {
            for object in initial_state_cfg_entry.objects {
                state.init_order_lock(object.to_object_reference()).await;
                state.insert_object(object).await;
            }
        }
        state
    });

    AuthorityServer::new(
        local_ip_addr.to_string(),
        server_config.authority.base_port,
        buffer_size,
        state,
    )
}

#[derive(StructOpt)]
#[structopt(
    name = "FastPay Server",
    about = "A byzantine fault tolerant payments sidechain with low-latency finality and high throughput"
)]
struct ServerOpt {
    /// Path to the file containing the server configuration of this FastPay authority (including its secret key)
    #[structopt(long)]
    server: String,

    /// Subcommands. Acceptable values are run and generate.
    #[structopt(subcommand)]
    cmd: ServerCommands,
}

#[derive(StructOpt)]
enum ServerCommands {
    /// Runs the FastPay authority")
    #[structopt(name = "run")]
    Run {
        /// Maximum size of datagrams received and sent (bytes)
        #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE)]
        buffer_size: usize,

        /// Path to the file containing the public description of all authorities in this FastPay committee
        #[structopt(long)]
        committee: String,

        /// Path to the file describing the initial user accounts
        #[structopt(long)]
        initial_accounts: String,
    },

    /// Generate a new server configuration and output its public description
    #[structopt(name = "generate")]
    Generate {
        /// Sets the public name of the host
        #[structopt(long)]
        host: String,

        /// Sets the port
        #[structopt(long)]
        port: u32,

        /// Sets the path to the database folder
        #[structopt(long, default_value = "")]
        database_path: String,
    },
}

fn main() {

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");

    let options = ServerOpt::from_args();

    let server_config_path = &options.server;

    match options.cmd {
        ServerCommands::Run {
            buffer_size,
            committee,
            initial_accounts,
        } => {
            // Run the server
            run_server(
                server_config_path, 
                committee, 
                initial_accounts, 
                buffer_size);
        }

        ServerCommands::Generate {
            host,
            port,
            database_path,
        } => {
            let server = create_server_config(host, port, database_path);
            server
                .write(server_config_path)
                .expect("Unable to write server config file");
            info!("Wrote server config file");
            server.authority.print();
        }
    }
}

// Shared functions for REST API & CLI 
pub fn create_server_config(host: String, port: u32, database_path: String) -> AuthorityServerConfig {
    let (address, key) = get_key_pair();
    let authority = AuthorityConfig {
        address,
        host,
        base_port: port,
        database_path,
    };
    AuthorityServerConfig { authority, key }
}

pub fn run_server(
    server_config_path: &String, 
    committee_config_path: String, 
    initial_accounts_config_path: String, 
    buffer_size: usize) {
    let server = make_server(
        "0.0.0.0", // Allow local IP address to be different from the public one.
        server_config_path,
        &committee_config_path,
        &initial_accounts_config_path,
        buffer_size,
    );
    let rt = Runtime::new().unwrap();
    let mut handles = Vec::new();
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
    rt.block_on(join_all(handles));
}
