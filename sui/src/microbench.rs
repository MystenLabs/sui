// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use anyhow::Error;
use bytes::Bytes;
use clap::*;
use futures::stream::StreamExt;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use rayon::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};
use sui_adapter::genesis;
use sui_core::authority_client::{AuthorityAPI, AuthorityClient};
use sui_core::{authority::*, authority_server::AuthorityServer};
use sui_network::{network::NetworkClient, transport};
use sui_types::batch::UpdateItem;
use sui_types::crypto::{get_key_pair, AuthoritySignature, KeyPair, PublicKeyBytes, Signature};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};
use tokio::runtime::Builder;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::{self};
use tokio::sync::oneshot;
use tokio::sync::oneshot::channel;
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

#[derive(Debug, Clone, Parser)]
#[clap(
    name = "Sui Microbenchmark",
    about = "Local test and microbenchmark of the Sui authorities"
)]
struct ClientServerBenchmark {
    /// Hostname
    #[clap(long, default_value = "127.0.0.1")]
    host: String,
    /// Path to the database
    #[clap(long, default_value = "")]
    db_dir: String,
    /// Base port number
    #[clap(long, default_value = "9555")]
    port: u16,
    /// Size of the Sui committee. Minimum size is 4 to tolerate one fault
    #[clap(long, default_value = "10")]
    committee_size: usize,
    /// Forces sending transactions one at a time
    #[clap(long)]
    single_operation: bool,
    /// Number of transactions to be sent in the benchmark
    #[clap(long, default_value = "100000")]
    num_transactions: usize,
    /// Timeout for sending queries (us)
    #[clap(long, default_value = "40000000")]
    send_timeout_us: u64,
    /// Timeout for receiving responses (us)
    #[clap(long, default_value = "40000000")]
    recv_timeout_us: u64,
    /// Maximum size of datagrams received and sent (bytes)
    #[clap(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE_STR)]
    buffer_size: usize,
    /// Which execution path to track. TransactionsAndCerts or TransactionsOnly or CertsOnly
    #[clap(long, default_value = "TransactionsAndCerts")]
    benchmark_type: BenchmarkType,
    /// Number of connections to the server
    #[clap(long, default_value = "0")]
    tcp_connections: usize,
    /// Number of database cpus
    #[clap(long, default_value = "1")]
    db_cpus: usize,
    /// Use Move orders
    #[clap(long)]
    use_move: bool,
    #[clap(long, default_value = "2000")]
    batch_size: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, EnumString)]
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

const MIN_COMMITTEE_SIZE: usize = 4;
const OBJECT_ID_OFFSET: usize = 10000;

fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let benchmark = ClientServerBenchmark::parse();
    assert!(
        benchmark.committee_size >= MIN_COMMITTEE_SIZE,
        "Found committee size of {:?}, but minimum committee size is {:?}",
        benchmark.committee_size,
        MIN_COMMITTEE_SIZE
    );
    assert_eq!(
        benchmark.num_transactions % benchmark.batch_size,
        0,
        "num_transactions must integer divide batch_size",
    );

    let (state, transactions) = benchmark.make_authority_and_transactions();

    // Make multi-threaded runtime for the authority
    let b = benchmark.clone();
    let connections = if benchmark.tcp_connections > 0 {
        benchmark.tcp_connections
    } else {
        num_cpus::get()
    };
    if benchmark.benchmark_type == BenchmarkType::TransactionsAndCerts {
        assert!(
            (benchmark.num_transactions * 2 / connections) % 2 == 0,
            "Each tx and their cert must be sent in order. Multiple TCP connections will break requests into chunks, and chunk size must be an even number so it doesn't break each tx and cert pair"
        );
    }

    let (server_ready_flag_tx, _server_ready_flag_rx) = broadcast::channel(16);
    let (bench_result_tx, bench_result_rx) = channel();
    let server_ready_flag_tx1 = server_ready_flag_tx.clone();

    // Start load gen. It will wait for server signal
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .worker_threads(usize::min(num_cpus::get(), 24))
            .build()
            .unwrap();
        runtime.block_on(benchmark.launch_load_generator(
            connections,
            transactions,
            &mut server_ready_flag_tx.clone().subscribe(),
            bench_result_tx,
        ));
    });

    // Start server and send signal
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = b.spawn_server(state).await;

            server_ready_flag_tx1
                .send(0)
                .expect("Unable to notify that server started");

            if let Err(err) = server.join().await {
                error!("Server ended with an error: {err}");
            }
        });
    });

    let _ = bench_result_rx.blocking_recv().unwrap();
}

impl ClientServerBenchmark {
    fn make_authority_and_transactions(&self) -> (AuthorityState, Vec<Bytes>) {
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

        // Seed user accounts.
        info!("Init Authority.");
        let (state, store_bis) = make_authority_state(
            &path,
            self.db_cpus as i32,
            &committee,
            &public_auth0,
            secret_auth0,
        );

        info!("Generate empty store with Genesis.");
        let (address, keypair) = get_key_pair();
        // Lets not collide with genesis objects.
        let object_id_offset = OBJECT_ID_OFFSET;
        // The batch-adjusted number of transactions
        let batch_tx_count = self.num_transactions / self.batch_size;
        // Only need one gas object per batch
        let account_gas_objects: Vec<_> = make_gas_objects(
            address,
            batch_tx_count,
            self.batch_size,
            object_id_offset,
            self.use_move,
        );

        // Bulk load objects
        let all_objects: Vec<_> = account_gas_objects
            .iter()
            .flat_map(|(objects, gas)| objects.iter().chain(std::iter::once(gas)))
            .collect();

        // Ensure multiples
        assert_eq!(
            all_objects
                .iter()
                .map(|o| o.id())
                .collect::<HashSet<_>>()
                .len(),
            batch_tx_count * (self.batch_size + 1)
        );
        // Insert the objects
        store_bis.bulk_object_insert(&all_objects[..]).unwrap();

        info!("Preparing transactions.");
        let transactions = make_serialized_transactions(
            address,
            keypair,
            &committee,
            &account_gas_objects,
            &keys,
            self.batch_size,
            self.use_move,
            self.benchmark_type,
        );

        (state, transactions)
    }

    async fn spawn_server(
        &self,
        state: AuthorityState,
    ) -> transport::SpawnedServer<AuthorityServer> {
        let server = AuthorityServer::new(self.host.clone(), self.port, self.buffer_size, state);
        server.spawn().await.unwrap()
    }

    async fn launch_load_generator(
        &self,
        connections: usize,
        transactions: Vec<Bytes>,
        server_ready_flag_rx: &mut broadcast::Receiver<i32>,
        bench_result_tx: oneshot::Sender<f64>,
    ) {
        // Wait for server time to be ready
        match server_ready_flag_rx.recv().await {
            Ok(_) => (),
            Err(e) => error!("Failed to receive server start signal. Error {:?}", e),
        }

        let transaction_len_factor = if self.benchmark_type == BenchmarkType::TransactionsAndCerts {
            2
        } else {
            1
        };
        let items_number = transactions.len() / transaction_len_factor;
        let mut elapsed_time: u128 = 0;
        let client = NetworkClient::new(
            self.host.clone(),
            self.port,
            self.buffer_size,
            Duration::from_micros(self.send_timeout_us),
            Duration::from_micros(self.recv_timeout_us),
        );

        // We spawn a second client that listens to the batch interface
        let client_batch = NetworkClient::new(
            self.host.clone(),
            self.port,
            self.buffer_size,
            Duration::from_micros(self.send_timeout_us),
            Duration::from_micros(self.recv_timeout_us),
        );

        let _batch_client_handle = tokio::task::spawn(async move {
            let authority_client = AuthorityClient::new(client_batch);

            let mut start = 0;

            loop {
                let receiver = authority_client
                    .handle_batch_stream(BatchInfoRequest {
                        start,
                        end: start + 10_000,
                    })
                    .await;

                if let Err(e) = &receiver {
                    error!("Listener error: {:?}", e);
                    break;
                }
                let mut receiver = receiver.unwrap();

                info!("Start batch listener at sequence: {}.", start);
                while let Some(item) = receiver.next().await {
                    match item {
                        Ok(BatchInfoResponseItem(UpdateItem::Transaction((
                            _tx_seq,
                            _tx_digest,
                        )))) => {
                            start = _tx_seq + 1;
                        }
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch))) => {
                            info!(
                                "Client received batch up to sequence {}",
                                _signed_batch.batch.next_sequence_number
                            );
                        }
                        Err(err) => {
                            error!("{:?}", err);
                            break;
                        }
                    }
                }
            }
        });

        info!("Sending requests.");
        if self.single_operation {
            // Send batches one by one
            let mut transactions: VecDeque<_> = transactions.into_iter().collect();
            while let Some(transaction) = transactions.pop_front() {
                if transactions.len() % 1000 == 0 {
                    info!("Process message {}...", transactions.len());
                }

                let time_start = Instant::now();
                let resp = client.send_recv_bytes(transaction.to_vec()).await;
                elapsed_time += time_start.elapsed().as_micros();
                check_transaction_response(resp.map_err(|e| e.into()));
            }
        } else {
            info!("Number of TCP connections: {connections}");

            // Send all batches at once
            let time_start = Instant::now();
            let responses = client
                .batch_send(transactions, connections, 0)
                .map(|x| x.unwrap())
                .concat()
                .await;
            elapsed_time = time_start.elapsed().as_micros();

            info!("Received {} responses.", responses.len(),);
            // Check the responses for errors
            for resp in &responses {
                check_transaction_response(deserialize_message(&(resp.as_ref().unwrap())[..]))
            }
        }
        let throughput =
            1_000_000.0 * (items_number as f64 * self.batch_size as f64) / (elapsed_time as f64);

        warn!(
            "Completed benchmark for {}\nTotal time: {}us, items: {}, tx/sec: {}",
            self.benchmark_type,
            elapsed_time,
            items_number * self.batch_size,
            throughput
        );
        bench_result_tx.send(throughput).unwrap();
    }
}

/// Create a transaction for object transfer
/// This can either use the Move path or the native path
fn make_transfer_transaction(
    object_ref: ObjectRef,
    recipient: SuiAddress,
    use_move: bool,
) -> SingleTransactionKind {
    if use_move {
        let framework_obj_ref = (
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );

        SingleTransactionKind::Call(MoveCall {
            package: framework_obj_ref,
            module: ident_str!("SUI").to_owned(),
            function: ident_str!("transfer").to_owned(),
            type_arguments: Vec::new(),
            object_arguments: vec![object_ref],
            shared_object_arguments: vec![],
            pure_arguments: vec![bcs::to_bytes(&AccountAddress::from(recipient)).unwrap()],
        })
    } else {
        SingleTransactionKind::Transfer(Transfer {
            recipient,
            object_ref,
        })
    }
}

/// Creates an object for use in the microbench
fn create_object(object_id: ObjectID, owner: SuiAddress, use_move: bool) -> Object {
    if use_move {
        Object::with_id_owner_gas_coin_object_for_testing(
            object_id,
            SequenceNumber::new(),
            owner,
            1,
        )
    } else {
        Object::with_id_owner_for_testing(object_id, owner)
    }
}

/// This builds, signs a cert and serializes it
fn make_serialized_cert(
    keys: &[(PublicKeyBytes, KeyPair)],
    committee: &Committee,
    tx: Transaction,
) -> Vec<u8> {
    // Make certificate
    let mut certificate = CertifiedTransaction::new(tx);
    for i in 0..committee.quorum_threshold() {
        let (pubx, secx) = keys.get(i).unwrap();
        let sig = AuthoritySignature::new(&certificate.transaction.data, secx);
        certificate.signatures.push((*pubx, sig));
    }

    let serialized_certificate = serialize_cert(&certificate);
    assert!(!serialized_certificate.is_empty());
    serialized_certificate
}

fn make_authority_state(
    store_path: &Path,
    db_cpus: i32,
    committee: &Committee,
    pubx: &PublicKeyBytes,
    secx: KeyPair,
) -> (AuthorityState, Arc<AuthorityStore>) {
    fs::create_dir(&store_path).unwrap();
    info!("Open database on path: {:?}", store_path.as_os_str());

    let mut opts = Options::default();
    opts.increase_parallelism(db_cpus);
    opts.set_write_buffer_size(256 * 1024 * 1024);
    opts.enable_statistics();
    opts.set_stats_dump_period_sec(5);
    opts.set_enable_pipelined_write(true);

    // NOTE: turn off the WAL, but is not guaranteed to
    // recover from a crash. Keep turned off to max safety,
    // but keep as an option if we periodically flush WAL
    // manually.
    // opts.set_manual_wal_flush(true);

    let store = Arc::new(AuthorityStore::open(store_path, Some(opts)));
    (
        Runtime::new().unwrap().block_on(async {
            AuthorityState::new(
                committee.clone(),
                *pubx,
                Arc::pin(secx),
                store.clone(),
                genesis::clone_genesis_compiled_modules(),
                &mut genesis::get_genesis_context(),
            )
            .await
        }),
        store,
    )
}

fn make_gas_objects(
    address: SuiAddress,
    tx_count: usize,
    batch_size: usize,
    obj_id_offset: usize,
    use_move: bool,
) -> Vec<(Vec<Object>, Object)> {
    (0..tx_count)
        .into_par_iter()
        .map(|x| {
            let mut objects = vec![];
            for i in 0..batch_size {
                let mut obj_id = [0; 20];
                obj_id[..8]
                    .clone_from_slice(&(obj_id_offset + x * batch_size + i).to_be_bytes()[..8]);
                objects.push(create_object(ObjectID::from(obj_id), address, use_move));
            }

            let mut gas_object_id = [0; 20];
            gas_object_id[8..16].clone_from_slice(&(obj_id_offset + x).to_be_bytes()[..8]);
            let gas_object = Object::with_id_owner_gas_coin_object_for_testing(
                ObjectID::from(gas_object_id),
                SequenceNumber::new(),
                address,
                2000000,
            );
            assert!(gas_object.version() == SequenceNumber::from(0));

            (objects, gas_object)
        })
        .collect()
}

fn make_serialized_transactions(
    address: SuiAddress,
    keypair: KeyPair,
    committee: &Committee,
    account_gas_objects: &[(Vec<Object>, Object)],
    keys: &[(PublicKeyBytes, KeyPair)],
    batch_size: usize,
    use_move: bool,
    benchmark_type: BenchmarkType,
) -> Vec<Bytes> {
    // Make one transaction per account
    // Depending on benchmark_type, this could be the Order and/or Confirmation.
    account_gas_objects
        .par_iter()
        .map(|(objects, gas_obj)| {
            let next_recipient: SuiAddress = get_key_pair().0;
            let mut single_kinds = vec![];
            for object in objects {
                single_kinds.push(make_transfer_transaction(
                    object.compute_object_reference(),
                    next_recipient,
                    use_move,
                ));
            }
            let gas_object_ref = gas_obj.compute_object_reference();
            let data = if batch_size == 1 {
                TransactionData::new(
                    TransactionKind::Single(single_kinds.into_iter().next().unwrap()),
                    address,
                    gas_object_ref,
                    10000,
                )
            } else {
                assert!(single_kinds.len() == batch_size, "Inconsistent batch size");
                TransactionData::new(
                    TransactionKind::Batch(single_kinds),
                    address,
                    gas_object_ref,
                    2000000,
                )
            };

            let signature = Signature::new(&data, &keypair);
            let transaction = Transaction::new(data, signature);

            // Serialize transaction
            let serialized_transaction = serialize_transaction(&transaction);
            assert!(!serialized_transaction.is_empty());

            let mut transactions: Vec<Bytes> = Vec::new();

            // If we only want to exercise the certificate path, we skip the transactions
            if benchmark_type != BenchmarkType::CertsOnly {
                transactions.push(serialized_transaction.into());
            }

            // If we only want to exercise the certificate path, we skip the certificates
            if benchmark_type != BenchmarkType::TransactionsOnly {
                // Make certificate
                transactions.push(make_serialized_cert(keys, committee, transaction).into());
            }

            transactions
        })
        .flatten()
        .collect()
}

fn check_transaction_response(reply_message: Result<SerializedMessage, Error>) {
    match reply_message {
        Ok(SerializedMessage::TransactionResp(res)) => {
            if let Some(e) = res.signed_effects {
                if matches!(e.effects.status, ExecutionStatus::Failure { .. }) {
                    info!("Execution Error {:?}", e.effects.status);
                }
            }
        }
        Err(err) => {
            error!("Received Error {:?}", err);
        }
        Ok(q) => error!("Received invalid response {:?}", q),
    };
}
