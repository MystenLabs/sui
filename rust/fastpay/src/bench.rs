// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

extern crate fastpay;
extern crate fastpay_core;
extern crate log;

use fastpay::{network, transport};
use fastpay_core::authority::*;
use fastpay_core::base_types::*;
use fastpay_core::committee::*;
use fastpay_core::messages::*;
use fastpay_core::serialize::*;

use bytes::Bytes;
use clap::{App, Arg};
use futures::stream::StreamExt;
use log::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::runtime::Builder;
use tokio::time;

use std::thread;

#[derive(Debug, Clone)]
struct ClientServerBenchmark {
    network_protocol: transport::NetworkProtocol,
    host: String,
    port: u32,
    committee_size: usize,
    num_shards: u32,
    max_in_flight: usize,
    num_accounts: usize,
    send_timeout: Duration,
    recv_timeout: Duration,
    buffer_size: usize,
    cross_shard_queue_size: usize,
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let benchmark = ClientServerBenchmark::from_command_line();

    let (states, orders) = benchmark.make_structures();

    // Start the servers on the thread pool
    for state in states {
        // Make special single-core runtime for each server
        let b = benchmark.clone();
        thread::spawn(move || {
            let mut runtime = Builder::new()
                .enable_all()
                .basic_scheduler()
                .thread_stack_size(15 * 1024 * 1024)
                .build()
                .unwrap();

            runtime.block_on(async move {
                let server = b.spawn_server(state).await;
                if let Err(err) = server.join().await {
                    error!("Server ended with an error: {}", err);
                }
            });
        });
    }

    let mut runtime = Builder::new()
        .enable_all()
        .basic_scheduler()
        .thread_stack_size(15 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(benchmark.launch_client(orders));
}

impl ClientServerBenchmark {
    fn make_structures(&self) -> (Vec<AuthorityState>, Vec<(u32, Bytes)>) {
        info!("Preparing accounts.");
        let mut keys = Vec::new();
        for _ in 0..self.committee_size {
            keys.push(get_key_pair());
        }
        let committee = Committee {
            voting_rights: keys.iter().map(|(k, _)| (*k, 1)).collect(),
            total_votes: self.committee_size,
        };

        // Pick an authority and create one state per shard.
        let (public_auth0, secret_auth0) = keys.pop().unwrap();
        let mut states = Vec::new();
        for i in 0..self.num_shards {
            let state = AuthorityState::new_shard(
                committee.clone(),
                public_auth0,
                secret_auth0.copy(),
                i as u32,
                self.num_shards,
            );
            states.push(state);
        }

        // Seed user accounts.
        let mut account_keys = Vec::new();
        for _ in 0..self.num_accounts {
            let keypair = get_key_pair();
            let i = AuthorityState::get_shard(self.num_shards, &keypair.0) as usize;
            assert!(states[i].in_shard(&keypair.0));
            let client = AccountOffchainState {
                balance: Balance::from(Amount::from(100)),
                next_sequence_number: SequenceNumber::from(0),
                pending_confirmation: None,
                confirmed_log: Vec::new(),
                synchronization_log: Vec::new(),
                received_log: Vec::new(),
            };
            states[i].accounts.insert(keypair.0, client);
            account_keys.push(keypair);
        }

        info!("Preparing transactions.");
        // Make one transaction per account (transfer order + confirmation).
        let mut orders: Vec<(u32, Bytes)> = Vec::new();
        let mut next_recipient = get_key_pair().0;
        for (pubx, secx) in account_keys.iter() {
            let transfer = Transfer {
                sender: *pubx,
                recipient: Address::FastPay(next_recipient),
                amount: Amount::from(50),
                sequence_number: SequenceNumber::from(0),
                user_data: UserData::default(),
            };
            next_recipient = *pubx;
            let order = TransferOrder::new(transfer.clone(), secx);
            let shard = AuthorityState::get_shard(self.num_shards, &pubx);

            // Serialize order
            let bufx = serialize_transfer_order(&order);
            assert!(!bufx.is_empty());

            // Make certificate
            let mut certificate = CertifiedTransferOrder {
                value: order,
                signatures: Vec::new(),
            };
            for i in 0..committee.quorum_threshold() {
                let (pubx, secx) = keys.get(i).unwrap();
                let sig = Signature::new(&certificate.value, secx);
                certificate.signatures.push((*pubx, sig));
            }

            let bufx2 = serialize_cert(&certificate);
            assert!(!bufx2.is_empty());

            orders.push((shard, bufx2.into()));
            orders.push((shard, bufx.into()));
        }

        (states, orders)
    }

    async fn spawn_server(&self, state: AuthorityState) -> transport::SpawnedServer {
        let server = network::Server::new(
            self.network_protocol,
            self.host.clone(),
            self.port,
            state,
            self.buffer_size,
            self.cross_shard_queue_size,
        );
        server.spawn().await.unwrap()
    }

    async fn launch_client(&self, mut orders: Vec<(u32, Bytes)>) {
        time::delay_for(Duration::from_millis(1000)).await;

        let items_number = orders.len() / 2;
        let time_start = Instant::now();

        let max_in_flight = (self.max_in_flight / self.num_shards as usize) as usize;
        info!("Set max_in_flight per shard to {}", max_in_flight);

        info!("Sending requests.");
        if self.max_in_flight > 0 {
            let mass_client = network::MassClient::new(
                self.network_protocol,
                self.host.clone(),
                self.port,
                self.buffer_size,
                self.send_timeout,
                self.recv_timeout,
                max_in_flight as u64,
            );
            let mut sharded_requests = HashMap::new();
            for (shard, buf) in orders.iter().rev() {
                sharded_requests
                    .entry(*shard)
                    .or_insert_with(Vec::new)
                    .push(buf.clone());
            }
            let responses = mass_client.run(sharded_requests).concat().await;
            info!("Received {} responses.", responses.len(),);
        } else {
            // Use actual client core
            let mut client = network::Client::new(
                self.network_protocol,
                self.host.clone(),
                self.port,
                self.num_shards,
                self.buffer_size,
                self.send_timeout,
                self.recv_timeout,
            );

            while !orders.is_empty() {
                if orders.len() % 1000 == 0 {
                    info!("Process message {}...", orders.len());
                }
                let (shard, order) = orders.pop().unwrap();
                let status = client.send_recv_bytes(shard, order.to_vec()).await;
                match status {
                    Ok(info) => {
                        debug!("Query response: {:?}", info);
                    }
                    Err(error) => {
                        error!("Failed to execute order: {}", error);
                    }
                }
            }
        }

        let time_total = time_start.elapsed().as_micros();
        warn!(
            "Total time: {}ms, items: {}, tx/sec: {}",
            time_total,
            items_number,
            1_000_000.0 * (items_number as f64) / (time_total as f64)
        );
    }

    fn from_command_line() -> Self {
        let matches = App::new("FastPay benchmark")
            .about("Local end-to-end test and benchmark of the FastPay protocol")
            .arg(
                Arg::with_name("protocol")
                    .long("protocol")
                    .help("Choose a network protocol between Udp and Tcp")
                    .default_value("Udp"),
            )
            .arg(
                Arg::with_name("host")
                    .long("host")
                    .help("Hostname")
                    .default_value("127.0.0.1"),
            )
            .arg(
                Arg::with_name("port")
                    .long("port")
                    .help("Base port number")
                    .default_value("9555"),
            )
            .arg(
                Arg::with_name("committee_size")
                    .long("committee_size")
                    .help("Size of the FastPay committee")
                    .default_value("10"),
            )
            .arg(
                Arg::with_name("num_shards")
                    .long("num_shards")
                    .help("Number of shards per FastPay authority")
                    .default_value("15"),
            )
            .arg(
                Arg::with_name("max_in_flight")
                    .long("max_in_flight")
                    .help("Maximum number of requests in flight (0 for blocking client)")
                    .default_value("1000"),
            )
            .arg(
                Arg::with_name("num_accounts")
                    .long("num_accounts")
                    .help("Number of accounts and transactions used in the benchmark")
                    .default_value("40000"),
            )
            .arg(
                Arg::with_name("send_timeout")
                    .long("send_timeout")
                    .help("Timeout for sending queries (us)")
                    .default_value("4000000"),
            )
            .arg(
                Arg::with_name("recv_timeout")
                    .long("recv_timeout")
                    .help("Timeout for receiving responses (us)")
                    .default_value("4000000"),
            )
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
                    .default_value("1"),
            )
            .get_matches();

        Self {
            network_protocol: matches.value_of("protocol").unwrap().parse().unwrap(),
            host: matches.value_of("host").unwrap().to_string(),
            port: matches.value_of("port").unwrap().parse().unwrap(),
            committee_size: matches.value_of("committee_size").unwrap().parse().unwrap(),
            num_shards: matches.value_of("num_shards").unwrap().parse().unwrap(),
            max_in_flight: matches.value_of("max_in_flight").unwrap().parse().unwrap(),
            num_accounts: matches.value_of("num_accounts").unwrap().parse().unwrap(),
            send_timeout: Duration::from_micros(
                matches.value_of("send_timeout").unwrap().parse().unwrap(),
            ),
            recv_timeout: Duration::from_micros(
                matches.value_of("recv_timeout").unwrap().parse().unwrap(),
            ),
            buffer_size: matches.value_of("buffer_size").unwrap().parse().unwrap(),
            cross_shard_queue_size: matches
                .value_of("cross_shard_queue_size")
                .unwrap()
                .parse()
                .unwrap(),
        }
    }
}
