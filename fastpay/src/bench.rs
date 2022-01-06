// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use bytes::Bytes;
use fastpay::config::UserAccount;
use fastpay::{network, transport};
use fastpay_core::authority::*;
use fastpay_core::client::Client;
use fastx_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};
use futures::stream::StreamExt;
use log::*;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::runtime::Runtime;
use tokio::{runtime::Builder, time};

use std::fs;
use std::sync::Arc;
use std::thread;
use std::{env, usize};
use strum_macros::EnumString;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "FastPay Benchmark",
    about = "Local end-to-end test and benchmark of the FastPay protocol"
)]
struct ClientServerBenchmark {
    /// Choose a network protocol between Udp and Tcp
    #[structopt(long, default_value = "udp")] // Seeing issue with TCP for end-to-end
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
    /// Which execution path to track. Options: OrdersAndCerts, OrdersOnly, CertsOnly, TransferResponseTime
    #[structopt(long, default_value = "OrdersAndCerts")]
    benchmark_type: BenchmarkType,
    /// Number of objects per account
    #[structopt(long, default_value = "10")]
    gas_objs_per_account: usize,
    /// Gas value per object
    #[structopt(long, default_value = "1000")]
    #[allow(dead_code)]
    value_per_per_obj: usize,
}
#[derive(Debug, Clone, PartialEq, EnumString)]
enum BenchmarkType {
    OrdersAndCerts,
    OrdersOnly,
    CertsOnly,
    TransferResponseTime,
}
impl std::fmt::Display for BenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

fn make_port(base_port: u32, offset: u32) -> u32 {
    base_port + offset as u32
}
fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let benchmark = ClientServerBenchmark::from_args();

    // Create user accounts
    let accounts = benchmark.make_populated_accounts(
        benchmark.num_accounts,
        benchmark.gas_objs_per_account,
        benchmark.value_per_per_obj,
    );

    // Collate all objects
    let mut all_objects = Vec::new();
    for (_, objs) in &accounts {
        all_objects.append(&mut objs.clone());
    }

    // Create authority states
    let (authority_states, committee) = benchmark.make_authority_states(all_objects.to_vec());

    // Make distinct ports per authority
    let mut port_table = HashMap::new();
    for (idx, state) in authority_states.iter().enumerate() {
        port_table.insert(state.name, make_port(benchmark.port, idx as u32));
    }

    // Create orders for mass transfer
    let orders = benchmark.make_mass_orders(&accounts, &committee, &authority_states);

    // Make multi-threaded runtime for authorities
    for state in authority_states {
        let b = benchmark.clone();
        let port = *port_table.get(&state.name).unwrap();

        thread::spawn(move || {
            let mut runtime = Builder::new()
                .enable_all()
                .threaded_scheduler()
                .thread_stack_size(15 * 1024 * 1024)
                .build()
                .unwrap();

            runtime.block_on(async move {
                let server = b.spawn_server(state, port).await;
                if let Err(err) = server.join().await {
                    error!("Server ended with an error: {}", err);
                }
            });
        });
    }
    let report = if benchmark.benchmark_type == BenchmarkType::TransferResponseTime {
        // Wait for servers to be ready
        thread::sleep(Duration::from_millis(6000));
        benchmark.launch_client_for_native_end_to_end_transfer(&accounts, &committee, &port_table)
    } else {
        // Make a single-core runtime for the client.
        let mut runtime = Builder::new()
            .enable_all()
            .basic_scheduler()
            .thread_stack_size(15 * 1024 * 1024)
            .build()
            .unwrap();
        runtime.block_on(benchmark.launch_client_for_batch(orders))
    };
    println!("Num_tx: {}, time: {}us", report.0, report.1);
}

#[allow(dead_code)]
impl ClientServerBenchmark {
    fn make_client_state(
        &self,
        account: &UserAccount,
        committee: &Committee,
        port_table: &HashMap<AuthorityName, u32>,
    ) -> fastpay_core::client::ClientState<network::Client> {
        let address = account.address;
        let mut authority_clients = std::collections::HashMap::new();

        for authority in &committee.voting_rights {
            let client = network::Client::new(
                self.protocol,
                self.host.clone(),
                *port_table.get(authority.0).unwrap(),
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
            );
            authority_clients.insert(*authority.0, client);
        }

        fastpay_core::client::ClientState::new(
            address,
            account.key.copy(),
            committee.clone(),
            authority_clients,
            account.sent_certificates.clone(),
            account.received_certificates.clone(),
            account.object_ids.clone(),
        )
    }

    fn make_populated_accounts(
        &self,
        num_accounts: usize,
        num_objs_per_acc: usize,
        _gas_value_per_obj: usize,
    ) -> Vec<(UserAccount, Vec<Object>)> {
        let mut accounts = Vec::new();

        for _ in 0..num_accounts {
            let mut account = UserAccount::new(Vec::new());
            let mut objs = Vec::new();
            for _ in 0..num_objs_per_acc {
                let obj = Object::with_id_owner_for_testing(ObjectID::random(), account.address);
                objs.push(obj);
            }
            account.from_objs(&objs);
            accounts.push((account, objs));
        }
        accounts
    }

    fn make_authority_states(&self, objects: Vec<Object>) -> (Vec<AuthorityState>, Committee) {
        // Create keys for authorities
        let mut authority_keys = Vec::new();
        for _ in 0..self.committee_size {
            authority_keys.push(get_key_pair());
        }
        let committee = Committee::new(authority_keys.iter().map(|(k, _)| (*k, 1)).collect());

        // Create states for each authority
        let mut authority_states = Vec::new();

        for authority_key in authority_keys {
            // Pick an authority and create state.
            let (public_auth0, secret_auth0) = authority_key;

            // Create a random directory to store the DB
            let dir = env::temp_dir();
            let path = dir.join(format!("DB_{:?}", ObjectID::random()));
            fs::create_dir(&path).unwrap();

            let store = Arc::new(AuthorityStore::open(path, None));
            let state =
                AuthorityState::new(committee.clone(), public_auth0, secret_auth0.copy(), store);

            // Seed user accounts.
            let mut rt = Runtime::new().unwrap();
            //let mut account_objects = Vec::new();
            rt.block_on(async {
                for obj in &objects {
                    state.init_order_lock(obj.to_object_reference()).await;
                    state.insert_object(obj.clone()).await;
                }
            });
            authority_states.push(state);
        }
        (authority_states, committee)
    }

    fn make_mass_orders(
        &self,
        accounts: &[(UserAccount, Vec<Object>)],
        committee: &Committee,
        authority_states: &[AuthorityState],
    ) -> Vec<Bytes> {
        // Make one transaction per account (transfer order + confirmation).
        let mut orders: Vec<Bytes> = Vec::new();
        let mut next_recipient = accounts.last().unwrap().0.address;

        for (acc, objects) in accounts {
            for obj in objects.iter() {
                let (account_addr, object_id, account_secret) = (acc.address, obj.id(), &acc.key);

                let transfer = Transfer {
                    object_id,
                    sender: account_addr,
                    recipient: Address::FastPay(next_recipient),
                    sequence_number: SequenceNumber::from(0),
                    user_data: UserData::default(),
                };
                next_recipient = account_addr;
                let order = Order::new_transfer(transfer, account_secret);

                // Serialize order
                let serialized_order = serialize_order(&order);
                assert!(!serialized_order.is_empty());

                // Make certificate
                let mut certificate = CertifiedOrder {
                    order,
                    signatures: Vec::new(),
                };
                for i in 0..committee.quorum_threshold() {
                    let (pubx, secx) = (
                        authority_states.get(i).unwrap().name,
                        &authority_states.get(i).unwrap().secret,
                    );

                    let sig = Signature::new(&certificate.order.kind, secx);
                    certificate.signatures.push((pubx, sig));
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
        }
        orders
    }

    async fn spawn_server(&self, state: AuthorityState, port: u32) -> transport::SpawnedServer {
        let server = network::Server::new(
            self.protocol,
            self.host.clone(),
            port,
            state,
            self.buffer_size,
        );
        server.spawn().await.unwrap()
    }

    /// Runs single mass client with single authority
    async fn launch_client_for_batch(&self, mut orders: Vec<Bytes>) -> (usize, u128) {
        time::delay_for(Duration::from_millis(1000)).await;
        let order_len_factor = if self.benchmark_type == BenchmarkType::OrdersAndCerts {
            2
        } else {
            1
        };
        let items_number = orders.len() / order_len_factor;
        let time_start = Instant::now();
        let connections: usize = num_cpus::get();
        let max_in_flight = self.max_in_flight / connections as usize;
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
                let status = client.send_recv_bytes(order.to_vec()).await;
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
        (items_number, time_total)
    }
    /// Simulates real world transfer
    fn launch_client_for_native_end_to_end_transfer(
        &self,
        accounts: &[(UserAccount, Vec<Object>)],
        committee: &Committee,
        port_table: &HashMap<PublicKeyBytes, u32>,
    ) -> (usize, u128) {
        let mut rng = rand::thread_rng();
        let mut sender_idx = rng.gen_range(0, accounts.len());
        let mut recipient_idx = rng.gen_range(0, accounts.len());

        // Check that we have objects to send from this sender
        while accounts.get(sender_idx).unwrap().0.object_ids.is_empty() {
            sender_idx = rng.gen_range(0, accounts.len());
        }
        // Ensure sender is not receipient
        while recipient_idx == sender_idx {
            recipient_idx = rng.gen_range(0, accounts.len());
        }
        // For point to point transfer
        let sender = &accounts.get(sender_idx).unwrap().0;
        let recipient = &accounts.get(recipient_idx).unwrap().0;

        // Pick the first object from sender
        // This is faulty is run multiple times because object_ids can be stale after transfer: https://github.com/MystenLabs/fastnft/issues/105
        // Need to fix by fetching valid object ids
        let object_id = sender.object_ids.iter().next().unwrap().0;

        let mut rt = Runtime::new().unwrap();
        let elapsed_time = rt.block_on(async move {
            let mut sender_client_state = self.make_client_state(sender, committee, port_table);

            let time_start = Instant::now();
            let cert = sender_client_state
                .transfer_to_fastpay(*object_id, recipient.address, UserData::default())
                .await
                .unwrap();
            let elapsed_time = time_start.elapsed().as_micros();
            let mut recipient_client_state =
                self.make_client_state(recipient, committee, port_table);
            recipient_client_state
                .receive_from_fastpay(cert)
                .await
                .unwrap();
            elapsed_time
        });
        (1, elapsed_time)
    }
}
