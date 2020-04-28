// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

extern crate clap;
extern crate env_logger;
extern crate fastpay;
extern crate fastpay_core;

use fastpay::config::*;
use fastpay::network;
use fastpay::transport;
use fastpay_core::authority::*;
use fastpay_core::base_types::*;
use fastpay_core::committee::Committee;

use clap::{App, Arg, SubCommand};
use futures::future::join_all;
use log::*;
use tokio::runtime::Runtime;

#[allow(clippy::too_many_arguments)]
fn make_shard_server(
    local_ip_addr: &str,
    server_config_path: &str,
    committee_config_path: &str,
    initial_accounts_config_path: &str,
    initial_balance: Balance,
    buffer_size: usize,
    cross_shard_queue_size: usize,
    shard: u32,
) -> network::Server {
    let server_config =
        AuthorityServerConfig::read(server_config_path).expect("Fail to read server config");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Fail to read committee config");
    let initial_accounts_config = InitialStateConfig::read(initial_accounts_config_path)
        .expect("Fail to read initial account config");

    let committee = Committee::new(committee_config.voting_rights());
    let num_shards = server_config.authority.num_shards;

    let mut state = AuthorityState::new_shard(
        committee,
        server_config.authority.address,
        server_config.key.copy(),
        shard,
        num_shards,
    );

    // Load initial states
    for address in &initial_accounts_config.addresses {
        if AuthorityState::get_shard(num_shards, address) != shard {
            continue;
        }
        let client = AccountOffchainState {
            balance: initial_balance,
            next_sequence_number: SequenceNumber::from(0),
            pending_confirmation: None,
            confirmed_log: Vec::new(),
            synchronization_log: Vec::new(),
            received_log: Vec::new(),
        };
        state.accounts.insert(*address, client);
    }

    network::Server::new(
        server_config.authority.network_protocol,
        local_ip_addr.to_string(),
        server_config.authority.base_port,
        state,
        buffer_size,
        cross_shard_queue_size,
    )
}

fn make_servers(
    local_ip_addr: &str,
    server_config_path: &str,
    committee_config_path: &str,
    initial_accounts_config_path: &str,
    initial_balance: Balance,
    buffer_size: usize,
    cross_shard_queue_size: usize,
) -> Vec<network::Server> {
    let server_config =
        AuthorityServerConfig::read(server_config_path).expect("Fail to read server config");
    let num_shards = server_config.authority.num_shards;

    let mut servers = Vec::new();
    for shard in 0..num_shards {
        servers.push(make_shard_server(
            local_ip_addr,
            server_config_path,
            committee_config_path,
            initial_accounts_config_path,
            initial_balance,
            buffer_size,
            cross_shard_queue_size,
            shard,
        ))
    }
    servers
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let matches = App::new("FastPay server")
        .about("A byzantine fault tolerant payments sidechain with low-latency finality and high throughput")
        .args_from_usage("
            --server=<PATH>  'Path to the file containing the server configuration of this FastPay authority (including its secret key)'
            ")
        .subcommand(SubCommand::with_name("run")
            .about("Runs a service for each shard of the FastPay authority")
            .arg(
                Arg::with_name("buffer_size")
                    .long("buffer_size")
                    .help("Maximum size of datagrams received and sent (bytes")
                    .default_value(transport::DEFAULT_MAX_DATAGRAM_SIZE),
            )
            .arg(
                Arg::with_name("cross_shard_queue_size")
                    .long("cross_shard_queue_size")
                    .help("Number of cross shards messages allowed before blocking the main server loop")
                    .default_value("1000"),
            )
            .args_from_usage("
            --committee=<PATH>   'Path to the file containing the public description of all authorities in this FastPay committee'
            --initial_accounts=<PATH>    'Path to the file describing the initial user accounts'
            --initial_balance=<INT>      'Path to the file describing the initial balance of user accounts'
            --shard=[INT]                'Runs a specific shard (from 0 to shards-1)'
            "))
        .subcommand(SubCommand::with_name("generate")
            .about("Generate a new server configuration and output its public description")
            .arg(
                Arg::with_name("protocol")
                    .long("protocol")
                    .help("Chooses a network protocol between Udp and Tcp")
                    .default_value("Udp"),
            )
            .args_from_usage("
            --host=<ADDRESS>      'Sets the public name of the host'
            --port=<PORT>         'Sets the base port, i.e. the port on which the server listens for the first shard'
            --shards=<SHARDS>     'Number of shards for this authority'"))
        .get_matches();

    match matches.subcommand() {
        ("run", Some(subm)) => {
            // Reading our own config
            let server_config_path = matches.value_of("server").unwrap();
            let committee_config_path = subm.value_of("committee").unwrap();
            let initial_accounts_config_path = subm.value_of("initial_accounts").unwrap();
            let initial_balance = Balance::from(
                subm.value_of("initial_balance")
                    .unwrap()
                    .parse::<i128>()
                    .unwrap(),
            );
            let buffer_size = subm
                .value_of("buffer_size")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let cross_shard_queue_size = subm
                .value_of("cross_shard_queue_size")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let specific_shard = if subm.is_present("shard") {
                let shard = subm.value_of("shard").unwrap();
                Some(shard.parse::<u32>().unwrap())
            } else {
                None
            };

            // Run the server
            let servers = if let Some(shard) = specific_shard {
                info!("Running shard number {}", shard);
                let server = make_shard_server(
                    "0.0.0.0", // Allow local IP address to be different from the public one.
                    server_config_path,
                    committee_config_path,
                    initial_accounts_config_path,
                    initial_balance,
                    buffer_size,
                    cross_shard_queue_size,
                    shard,
                );
                vec![server]
            } else {
                info!("Running all shards");
                make_servers(
                    "0.0.0.0", // Allow local IP address to be different from the public one.
                    server_config_path,
                    committee_config_path,
                    initial_accounts_config_path,
                    initial_balance,
                    buffer_size,
                    cross_shard_queue_size,
                )
            };

            let mut rt = Runtime::new().unwrap();
            let mut handles = Vec::new();
            for server in servers {
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
            rt.block_on(join_all(handles));
        }
        ("generate", Some(subm)) => {
            let network_protocol = subm.value_of("protocol").unwrap().parse().unwrap();
            let server_config_path = matches.value_of("server").unwrap();
            let (address, key) = get_key_pair();
            let authority = AuthorityConfig {
                network_protocol,
                address,
                host: subm.value_of("host").unwrap().parse().unwrap(),
                base_port: subm.value_of("port").unwrap().parse().unwrap(),
                num_shards: subm.value_of("shards").unwrap().parse().unwrap(),
            };
            let server = AuthorityServerConfig { authority, key };
            server
                .write(server_config_path)
                .expect("Unable to write server config file");
            info!("Wrote server config file");
            server.authority.print();
        }
        _ => {
            error!("Unknown command");
        }
    }
}
