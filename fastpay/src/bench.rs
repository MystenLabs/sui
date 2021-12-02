// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::{network, transport};
use fastpay_core::{authority::*, base_types::*, committee::*, messages::*, serialize::*};

use bytes::Bytes;
use futures::stream::StreamExt;
use log::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use structopt::StructOpt;
use tokio::{runtime::Builder, time};

use std::thread;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "FastPay Benchmark",
    about = "Local end-to-end test and benchmark of the FastPay protocol"
)]
struct ClientServerBenchmark {
    /// Choose a network protocol between Udp and Tcp
    #[structopt(long, default_value = "udp")]
    protocol: transport::NetworkProtocol,
    /// Hostname
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,
    /// Base port number
    #[structopt(long, default_value = "9555")]
    port: u32,
    /// Size of the FastPay committee
    #[structopt(long, default_value = "10")]
    committee_size: usize,
    /// Number of shards per FastPay authority
    #[structopt(long, default_value = "15")]
    num_shards: u32,
    /// Maximum number of requests in flight (0 for blocking client)
    #[structopt(long, default_value = "1000")]
    max_in_flight: usize,
    /// Number of accounts and transactions used in the benchmark
    #[structopt(long, default_value = "40000")]
    num_accounts: usize,
    /// Timeout for sending queries (us)
    #[structopt(long, default_value = "4000000")]
    send_timeout_us: u64,
    /// Timeout for receiving responses (us)
    #[structopt(long, default_value = "4000000")]
    recv_timeout_us: u64,
    /// Maximum size of datagrams received and sent (bytes)
    #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE)]
    buffer_size: usize,
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let benchmark = ClientServerBenchmark::from_args();

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
            let object_id: ObjectID = address_to_object_id_hack(keypair.0);
            let i = AuthorityState::get_shard(self.num_shards, &object_id) as usize;
            assert!(states[i].in_shard(&object_id));
            let client = ObjectState {
                id: object_id,
                contents: Vec::new(),
                owner: keypair.0,
                next_sequence_number: SequenceNumber::from(0),
            };
            states[i].insert_object(client);
            account_keys.push(keypair);
        }

        info!("Preparing transactions.");
        // Make one transaction per account (transfer order + confirmation).
        let mut orders: Vec<(u32, Bytes)> = Vec::new();
        let mut next_recipient = get_key_pair().0;
        for (pubx, secx) in account_keys.iter() {
            let transfer = Transfer {
                object_id: address_to_object_id_hack(*pubx),
                sender: *pubx,
                recipient: Address::FastPay(next_recipient),
                sequence_number: SequenceNumber::from(0),
                user_data: UserData::default(),
            };
            next_recipient = *pubx;
            let order = TransferOrder::new(transfer.clone(), secx);
            let shard = AuthorityState::get_shard(self.num_shards, &order.transfer.object_id);

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
                let sig = Signature::new(&certificate.value.transfer, secx);
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
            self.protocol,
            self.host.clone(),
            self.port,
            state,
            self.buffer_size,
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
                self.protocol,
                self.host.clone(),
                self.port,
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
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
                self.protocol,
                self.host.clone(),
                self.port,
                self.num_shards,
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
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
}
