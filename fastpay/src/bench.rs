// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use bytes::Bytes;
use fastpay::{network, transport};
use fastpay_core::authority::*;
use fastx_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};
use futures::stream::StreamExt;
use log::*;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::runtime::Runtime;
use tokio::{runtime::Builder, time};

use std::env;
use std::fs;
use std::sync::Arc;
use std::thread;
use strum_macros::EnumString;
use rocksdb::Options;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "FastPay Benchmark",
    about = "Local end-to-end test and benchmark of the FastPay protocol"
)]
struct ClientServerBenchmark {
    /// Choose a network protocol between Udp and Tcp
    #[structopt(long, default_value = "tcp")]
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
    /// Which execution path to track. OrdersAndCerts or OrdersOnly or CertsOnly
    #[structopt(long, default_value = "OrdersAndCerts")]
    benchmark_type: BenchmarkType,
}
#[derive(Debug, Clone, PartialEq, EnumString)]
enum BenchmarkType {
    OrdersAndCerts,
    OrdersOnly,
    CertsOnly,
}
impl std::fmt::Display for BenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let benchmark = ClientServerBenchmark::from_args();
    let (state, orders) = benchmark.make_structures();

    // Make multi-threaded runtime for the authority
    let b = benchmark.clone();
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
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

    // Make a single-core runtime for the client.
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(15 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(benchmark.launch_client(orders));
}

impl ClientServerBenchmark {
    fn make_structures(&self) -> (AuthorityState, Vec<Bytes>) {
        info!("Starting benchmark: {}", self.benchmark_type);
        info!("Preparing accounts.");
        let mut keys = Vec::new();
        for _ in 0..self.committee_size {
            keys.push(get_key_pair());
        }
        let committee = Committee::new(keys.iter().map(|(k, _)| (*k, 1)).collect());

        // Pick an authority and create state.
        let (public_auth0, secret_auth0) = keys.pop().unwrap();

        // Create a random directory to store the DB
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();


        let mut opts = Options::default();
        opts.increase_parallelism(4);
        let store = Arc::new(AuthorityStore::open(path, None));

        // Seed user accounts.
        let rt = Runtime::new().unwrap();
        let mut account_objects = Vec::new();
        let mut gas_objects = Vec::new();
        let state = rt.block_on(async {
            let state = AuthorityState::new_with_genesis_modules(
                committee.clone(),
                public_auth0,
                secret_auth0.copy(),
                store,
            )
            .await;
            for _ in 0..self.num_accounts {
                let keypair = get_key_pair();
                let object_id: ObjectID = ObjectID::random();

                let object = Object::with_id_owner_for_testing(object_id, keypair.0);
                assert!(object.version() == SequenceNumber::from(0));
                let object_ref = object.to_object_reference();
                state.init_order_lock(object_ref).await;
                state.insert_object(object).await;
                account_objects.push((keypair.0, object_ref, keypair.1));

                let gas_object_id = ObjectID::random();
                let gas_object = Object::with_id_owner_for_testing(gas_object_id, keypair.0);
                assert!(gas_object.version() == SequenceNumber::from(0));
                let gas_object_ref = gas_object.to_object_reference();
                state.init_order_lock(gas_object_ref).await;
                state.insert_object(gas_object).await;
                gas_objects.push(gas_object_ref);
            }
            state
        });

        info!("Preparing transactions.");
        // Make one transaction per account (transfer order + confirmation).
        let mut orders: Vec<Bytes> = Vec::new();
        let mut next_recipient = get_key_pair().0;
        for ((pubx, object_ref, secx), gas_payment) in account_objects.iter().zip(gas_objects) {
            let transfer = Transfer {
                object_ref: *object_ref,
                sender: *pubx,
                recipient: Address::FastPay(next_recipient),
                gas_payment,
            };
            next_recipient = *pubx;
            let order = Order::new_transfer(transfer, secx);

            // Serialize order
            let serialized_order = serialize_order(&order);
            assert!(!serialized_order.is_empty());

            // Make certificate
            let mut certificate = CertifiedOrder {
                order,
                signatures: Vec::new(),
            };
            for i in 0..committee.quorum_threshold() {
                let (pubx, secx) = keys.get(i).unwrap();
                let sig = Signature::new(&certificate.order.kind, secx);
                certificate.signatures.push((*pubx, sig));
            }

            let serialized_certificate = serialize_cert(&certificate);
            assert!(!serialized_certificate.is_empty());

            if self.benchmark_type != BenchmarkType::OrdersOnly {
                orders.push(serialized_order.into());
            }
            if self.benchmark_type != BenchmarkType::CertsOnly {
                orders.push(serialized_certificate.into());
            }
        }

        (state, orders)
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

    async fn launch_client(&self, mut orders: Vec<Bytes>) {
        time::sleep(Duration::from_millis(1000)).await;
        let order_len_factor = if self.benchmark_type == BenchmarkType::OrdersAndCerts {
            2
        } else {
            1
        };
        let items_number = orders.len() / order_len_factor;
        let time_start = Instant::now();

        let connections: usize = num_cpus::get();
        let max_in_flight = self.max_in_flight / connections as usize;
        info!("Number of TCP connections: {}", connections);
        info!("Set max_in_flight to {}", max_in_flight);

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

            let responses = mass_client.run(orders, connections).concat().await;
            info!("Received {} responses.", responses.len(),);
        } else {
            // Use actual client core
            let client = network::Client::new(
                self.protocol,
                self.host.clone(),
                self.port,
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
            );

            while !orders.is_empty() {
                if orders.len() % 1000 == 0 {
                    info!("Process message {}...", orders.len());
                }
                let order = orders.pop().unwrap();
                let status = client
                    .send_recv_bytes(order.to_vec(), object_info_deserializer)
                    .await;
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
            "Completed benchmark for {}\nTotal time: {}us, items: {}, tx/sec: {}",
            self.benchmark_type,
            time_total,
            items_number,
            1_000_000.0 * (items_number as f64) / (time_total as f64)
        );
    }
}
