// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::{config::*, network, transport};
use fastpay_core::authority::*;
use fastx_types::{base_types::*, committee::Committee, object::Object};

use futures::future::join_all;
use log::*;
use structopt::StructOpt;
use tokio::runtime::Runtime;

#[allow(clippy::too_many_arguments)]
fn make_server(
    local_ip_addr: &str,
    server_config_path: &str,
    committee_config_path: &str,
    initial_accounts_config_path: &str,
    buffer_size: usize,
) -> network::Server {
    let server_config =
        AuthorityServerConfig::read(server_config_path).expect("Fail to read server config");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Fail to read committee config");
    let initial_accounts_config = InitialStateConfig::read(initial_accounts_config_path)
        .expect("Fail to read initial account config");

    let committee = Committee::new(committee_config.voting_rights());

    let state = AuthorityState::new(
        committee,
        server_config.authority.address,
        server_config.key.copy(),
    );

    // Load initial states
    for (address, object_id) in &initial_accounts_config.accounts {
        let mut client = Object::with_id_for_testing(*object_id);
        client.transfer(*address);
        state.insert_object(client);
    }

    network::Server::new(
        server_config.authority.network_protocol,
        local_ip_addr.to_string(),
        server_config.authority.base_port,
        state,
        buffer_size,
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
    /// Runs a service for each shard of the FastPay authority")
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
        /// Chooses a network protocol between Udp and Tcp
        #[structopt(long, default_value = "Udp")]
        protocol: transport::NetworkProtocol,

        /// Sets the public name of the host
        #[structopt(long)]
        host: String,

        /// Sets the base port, i.e. the port on which the server listens for the first shard
        #[structopt(long)]
        port: u32,

        /// Sets the public name of the host
        #[structopt(long)]
        database_path: String,
    },
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = ServerOpt::from_args();

    let server_config_path = &options.server;

    match options.cmd {
        ServerCommands::Run {
            buffer_size,
            committee,
            initial_accounts,
        } => {
            // Run the server

                    let server = make_server(
                        "0.0.0.0", // Allow local IP address to be different from the public one.
                        server_config_path,
                        &committee,
                        &initial_accounts,
                        buffer_size,
                    );


            let mut rt = Runtime::new().unwrap();
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

        ServerCommands::Generate {
            protocol,
            host,
            port,
            database_path,
        } => {
            let (address, key) = get_key_pair();
            let authority = AuthorityConfig {
                network_protocol: protocol,
                address,
                host,
                base_port: port,
                database_path,
            };
            let server = AuthorityServerConfig { authority, key };
            server
                .write(server_config_path)
                .expect("Unable to write server config file");
            info!("Wrote server config file");
            server.authority.print();
        }
    }
}
