// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use bytes::Bytes;
use futures::stream::StreamExt;
use move_core_types::ident_str;
use rand::rngs::StdRng;
use rand::Rng;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use sui_adapter::genesis;
use sui_core::{authority::*, authority_server::AuthorityServer};
use sui_network::{network::NetworkClient, transport};
use sui_types::crypto::{get_key_pair, AuthoritySignature, Signature};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};
use tokio::runtime::Runtime;
use tokio::{runtime::Builder, time};
use tracing::subscriber::set_global_default;
use tracing::*;
use tracing_subscriber::EnvFilter;

use rocksdb::Options;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use strum_macros::EnumString;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "Sui Benchmark",
    about = "Local end-to-end test and benchmark of the Sui protocol"
)]
struct ClientServerBenchmark {
    /// Hostname
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,
    /// Path to the database
    #[structopt(long, default_value = "")]
    db_dir: String,
    /// Base port number
    #[structopt(long, default_value = "9555")]
    port: u16,
    /// Size of the Sui committee
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
    #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE_STR)]
    buffer_size: usize,
    /// Which execution path to track. TransactionsAndCerts or TransactionsOnly or CertsOnly
    #[structopt(long, default_value = "TransactionsAndCerts")]
    benchmark_type: BenchmarkType,
    /// Number of connections to the server
    #[structopt(long, default_value = "0")]
    tcp_connections: usize,
    /// Number of database cpus
    #[structopt(long, default_value = "1")]
    db_cpus: usize,
    /// Use Move orders
    #[structopt(long)]
    use_move: bool,
}
#[derive(Debug, Clone, PartialEq, EnumString)]
enum BenchmarkType {
    TransactionsAndCerts,
    TransactionsOnly,
    CertsOnly,
}
impl std::fmt::Display for BenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let benchmark = ClientServerBenchmark::from_args();
    let (state, transactions) = benchmark.make_structures();

    // Make multi-threaded runtime for the authority
    let b = benchmark.clone();
    let connections = if benchmark.tcp_connections > 0 {
        benchmark.tcp_connections
    } else {
        num_cpus::get()
    };

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
    runtime.block_on(benchmark.launch_client(connections, transactions));
}

impl ClientServerBenchmark {
    fn make_structures(&self) -> (AuthorityState, Vec<Bytes>) {
        info!("Starting benchmark: {}", self.benchmark_type);
        info!("Preparing accounts.");
        let mut keys = Vec::new();
        for _ in 0..self.committee_size {
            let (_, key_pair) = get_key_pair();
            let name = *key_pair.public_key_bytes();
            keys.push((name, key_pair));
        }
        let committee = Committee::new(keys.iter().map(|(k, _)| (*k, 1)).collect());

        // Pick an authority and create state.
        let (public_auth0, secret_auth0) = keys.pop().unwrap();

        // Create a random directory to store the DB
        let path = if self.db_dir.is_empty() {
            let dir = env::temp_dir();
            dir.join(format!("DB_{:?}", ObjectID::random()))
        } else {
            let dir = Path::new(&self.db_dir);
            dir.join(format!("DB_{:?}", ObjectID::random()))
        };
        fs::create_dir(&path).unwrap();
        info!("Open database on path: {:?}", path.as_os_str());

        let mut opts = Options::default();
        opts.increase_parallelism(self.db_cpus as i32);
        let store = Arc::new(AuthorityStore::open(path, None));

        // Seed user accounts.
        let rt = Runtime::new().unwrap();
        let mut account_objects = Vec::new();
        let mut gas_objects = Vec::new();
        let state = rt.block_on(async {
            let state = AuthorityState::new(
                committee.clone(),
                public_auth0,
                Arc::pin(secret_auth0),
                store,
                genesis::clone_genesis_compiled_modules(),
                &mut genesis::get_genesis_context(),
            )
            .await;
            let mut rnd = <StdRng as rand::SeedableRng>::seed_from_u64(0);
            for _ in 0..self.num_accounts {
                let (address, keypair) = get_key_pair();

                let object_id: ObjectID = ObjectID::random();
                let object = if self.use_move {
                    Object::with_id_owner_gas_coin_object_for_testing(
                        ObjectID::random(),
                        SequenceNumber::new(),
                        address,
                        rnd.gen::<u64>(),
                    )
                } else {
                    Object::with_id_owner_for_testing(object_id, address)
                };

                assert!(object.version() == SequenceNumber::from(0));
                let object_ref = object.to_object_reference();
                state.init_transaction_lock(object_ref).await;
                account_objects.push((address, object.clone(), keypair));
                state.insert_object(object).await;

                let gas_object_id = ObjectID::random();
                let gas_object = Object::with_id_owner_for_testing(gas_object_id, address);
                assert!(gas_object.version() == SequenceNumber::from(0));
                let gas_object_ref = gas_object.to_object_reference();
                state.init_transaction_lock(gas_object_ref).await;
                gas_objects.push(gas_object.clone());
                state.insert_object(gas_object).await;
            }
            state
        });

        info!("Preparing transactions.");
        // Make one transaction per account (transfer transaction + confirmation).
        let mut transactions: Vec<Bytes> = Vec::new();
        let mut next_recipient: SuiAddress = get_key_pair().0;
        for ((account_addr, object, secret), gas_obj) in account_objects.iter().zip(gas_objects) {
            let object_ref = object.to_object_reference();
            let gas_object_ref = gas_obj.to_object_reference();

            let data = if self.use_move {
                // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
                let framework_obj_ref = (
                    ObjectID::from(SUI_FRAMEWORK_ADDRESS),
                    SequenceNumber::new(),
                    ObjectDigest::new([0; 32]),
                );

                TransactionData::new_move_call(
                    *account_addr,
                    framework_obj_ref,
                    ident_str!("GAS").to_owned(),
                    ident_str!("transfer").to_owned(),
                    Vec::new(),
                    gas_object_ref,
                    vec![object_ref],
                    vec![],
                    vec![bcs::to_bytes(&next_recipient.to_vec()).unwrap()],
                    1000,
                )
            } else {
                TransactionData::new_transfer(
                    next_recipient,
                    object_ref,
                    *account_addr,
                    gas_object_ref,
                )
            };
            let signature = Signature::new(&data, secret);
            let transaction = Transaction::new(data, signature);

            // Set the next recipient to current
            next_recipient = *account_addr;

            // Serialize transaction
            let serialized_transaction = serialize_transaction(&transaction);
            assert!(!serialized_transaction.is_empty());

            // Make certificate
            let mut certificate = CertifiedTransaction {
                transaction,
                signatures: Vec::new(),
            };
            for i in 0..committee.quorum_threshold() {
                let (pubx, secx) = keys.get(i).unwrap();
                let sig = AuthoritySignature::new(&certificate.transaction.data, secx);
                certificate.signatures.push((*pubx, sig));
            }

            let serialized_certificate = serialize_cert(&certificate);
            assert!(!serialized_certificate.is_empty());

            if self.benchmark_type != BenchmarkType::CertsOnly {
                transactions.push(serialized_transaction.into());
            }
            if self.benchmark_type != BenchmarkType::TransactionsOnly {
                transactions.push(serialized_certificate.into());
            }
        }

        (state, transactions)
    }

    async fn spawn_server(&self, state: AuthorityState) -> transport::SpawnedServer {
        let server = AuthorityServer::new(self.host.clone(), self.port, self.buffer_size, state);
        server.spawn().await.unwrap()
    }

    async fn launch_client(&self, connections: usize, mut transactions: Vec<Bytes>) {
        // Give the server time to be ready
        time::sleep(Duration::from_millis(1000)).await;

        let transaction_len_factor = if self.benchmark_type == BenchmarkType::TransactionsAndCerts {
            2
        } else {
            1
        };
        let items_number = transactions.len() / transaction_len_factor;
        let mut elapsed_time: u128 = 0;

        let max_in_flight = self.max_in_flight / connections as usize;
        info!("Number of TCP connections: {}", connections);
        info!("Set max_in_flight to {}", max_in_flight);

        info!("Sending requests.");
        if self.max_in_flight > 0 {
            let mass_client = NetworkClient::new(
                self.host.clone(),
                self.port,
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
            );

            let time_start = Instant::now();
            let responses = mass_client
                .batch_send(transactions, connections, max_in_flight as u64)
                .concat()
                .await;
            elapsed_time = time_start.elapsed().as_micros();

            info!("Received {} responses.", responses.len(),);
            // Check the responses for errors
            for resp in &responses {
                let reply_message = deserialize_message(&resp[..]);
                match reply_message {
                    Ok(SerializedMessage::TransactionResp(res)) => {
                        if let Some(e) = res.signed_effects {
                            if matches!(e.effects.status, ExecutionStatus::Failure { .. }) {
                                info!("Execution Error {:?}", e.effects.status);
                            }
                        }
                    }
                    Err(err) => {
                        info!("Received Error {:?}", err);
                    }
                    _ => {}
                };
            }
        } else {
            // Use actual client core
            let client = NetworkClient::new(
                self.host.clone(),
                self.port,
                self.buffer_size,
                Duration::from_micros(self.send_timeout_us),
                Duration::from_micros(self.recv_timeout_us),
            );

            while !transactions.is_empty() {
                if transactions.len() % 1000 == 0 {
                    info!("Process message {}...", transactions.len());
                }
                let transaction = transactions.pop().unwrap();

                let time_start = Instant::now();
                let resp = client.send_recv_bytes(transaction.to_vec()).await;
                elapsed_time += time_start.elapsed().as_micros();
                let status = deserialize_object_info(resp.unwrap());

                match status {
                    Ok(info) => {
                        debug!("Query response: {:?}", info);
                    }
                    Err(error) => {
                        error!("Failed to execute transaction: {}", error);
                    }
                }
            }
        }

        warn!(
            "Completed benchmark for {}\nTotal time: {}us, items: {}, tx/sec: {}",
            self.benchmark_type,
            elapsed_time,
            items_number,
            1_000_000.0 * (items_number as f64) / (elapsed_time as f64)
        );
    }
}
