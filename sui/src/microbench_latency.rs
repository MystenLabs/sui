// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use anyhow::Error;
use bytes::Bytes;
use futures::channel::mpsc::{channel as MpscChannel, Sender as MpscSender};
use futures::stream::StreamExt;
use futures::SinkExt;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use rayon::prelude::*;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use sui_adapter::genesis;
use sui_core::{authority::*, authority_server::AuthorityServer};
use sui_network::{network::NetworkClient, transport};
use sui_types::crypto::{get_key_pair, AuthoritySignature, KeyPair, PublicKeyBytes, Signature};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};
use tokio::runtime::Builder;
use tokio::runtime::Runtime;

use tokio::sync::Notify;
use tokio::time;
use tracing::subscriber::set_global_default;
use tracing::*;
use tracing_subscriber::EnvFilter;

use rocksdb::Options;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "Sui Latency Microbenchmark",
    about = "Local test and latency microbenchmark of the Sui authorities"
)]
struct FixedRateBenchmark {
    /// Hostname
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,
    /// Base port number
    #[structopt(long, default_value = "9550")]
    port: u16,
    /// Size of the Sui committee. Minimum size is 4 to tolerate one fault
    #[structopt(long, default_value = "10")]
    committee_size: usize,

    /// Timeout for sending queries (us)
    #[structopt(long, default_value = "10000000")]
    send_timeout_us: u64,
    /// Timeout for receiving responses (us)
    #[structopt(long, default_value = "10000000")]
    recv_timeout_us: u64,
    /// Maximum size of datagrams received and sent (bytes)
    #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE_STR)]
    buffer_size: usize,
    /// Number of connections to the server
    #[structopt(long, default_value = "0")]
    tcp_connections: usize,
    #[structopt(long, default_value = "1")]
    db_cpus: usize,
    /// Use Move orders
    #[structopt(long)]
    use_move: bool,

    /// Number of chunks to send
    #[structopt(long, default_value = "100")]
    num_chunks: usize,
    /// Size of chunks per tick
    #[structopt(long, default_value = "1000")]
    chunk_size: usize,
    /// The time between each tick. Default 10ms
    #[structopt(long, default_value = "10000")]
    period_us: u64,
}

const MIN_COMMITTEE_SIZE: usize = 4;
const OBJECT_ID_OFFSET: usize = 10000;

fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let benchmark = FixedRateBenchmark::from_args();
    assert!(
        benchmark.committee_size >= MIN_COMMITTEE_SIZE,
        "Found committee size of {:?}, but minimum committee size is {:?}",
        benchmark.committee_size,
        MIN_COMMITTEE_SIZE
    );

    let b = benchmark.clone();
    let connections = if benchmark.tcp_connections > 0 {
        benchmark.tcp_connections
    } else {
        num_cpus::get()
    };
    info!("Starting latency benchmark");

    let (state, transactions) = benchmark.make_authority_and_transactions(connections);

    assert!(
        (benchmark.chunk_size * benchmark.num_chunks * 2 / connections) % 2 == 0,
        "Each tx and their cert must be sent in order. Multiple TCP connections will break requests into chunks, and chunk size must be an even number so it doesn't break each tx and cert pair"
    );

    // Server uses this to notify that it is ready
    let server_started_notifier_rx = Arc::new(Notify::new());
    let server_started_notifier_tx = server_started_notifier_rx.clone();

    // Start server and send signal
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = b.spawn_server(state).await;
            info!("Server ready");
            // Notify generator
            server_started_notifier_tx.notify_one();

            if let Err(err) = server.join().await {
                error!("Server ended with an error: {err}");
            }
        });
    });

    // Wait for server to start
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();
    runtime.block_on(server_started_notifier_rx.notified());

    // Start load gen
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();
    let results = runtime
        .block_on(benchmark.launch_periodic_load_generator(transactions, benchmark.period_us));

    println!("{:?}", results);
}

impl FixedRateBenchmark {
    fn make_authority_and_transactions(&self, conn: usize) -> (AuthorityState, Vec<Bytes>) {
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
        let path = env::temp_dir().join(format!("DB_{:?}", ObjectID::random()));

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

        assert_eq!(self.chunk_size % conn, 0);
        // batch_size_per_conn = ceiling(chunk_size / conn)
        let batch_size_per_conn = self.chunk_size / conn;

        // The batch-adjusted number of transactions
        let batch_tx_count = self.num_chunks * conn;

        // Only need one gas object per batch
        let account_gas_objects: Vec<_> = make_gas_objects(
            address,
            batch_tx_count,
            batch_size_per_conn,
            object_id_offset,
            self.use_move,
        );

        // Bulk load objects
        let all_objects: Vec<_> = account_gas_objects
            .iter()
            .flat_map(|(objects, gas)| objects.iter().chain(std::iter::once(gas)))
            .collect();

        // Insert the objects
        store_bis.bulk_object_insert(&all_objects[..]).unwrap();

        info!("Preparing transactions.");
        let transactions = make_serialized_transactions(
            address,
            keypair,
            &committee,
            &account_gas_objects,
            &keys,
            batch_size_per_conn,
            self.use_move,
        );

        (state, transactions)
    }

    async fn launch_periodic_load_generator(
        &self,
        transactions: Vec<Bytes>,
        period_us: u64,
    ) -> Vec<u128> {
        info!(
            "Running periodic load generator: {} chunks, {} txes/chunk, interval {}us",
            self.num_chunks, self.chunk_size, period_us
        );

        let mut idx = 0;
        let _host = self.host.clone();
        let port = self.port;
        let buffer_size = self.buffer_size;
        let recv_timeout_us = self.recv_timeout_us;
        let send_timeout_us = self.send_timeout_us;

        let mut handles = vec![];
        let notifier = Arc::new(Notify::new());

        let (result_chann_tx, mut rx) = MpscChannel(transactions.len() * 2);

        let conn = num_cpus::get();
        // Spin up a bunch of worker tasks
        // Give each task
        // Step by 2*conn due to order+confirmation, with `conn` tcp connections
        // Take up to 2*conn for each task

        for tx_chunk in transactions[..].chunks(2 * conn) {
            let notif = notifier.clone();
            let mut result_chann_tx = result_chann_tx.clone();
            let host = self.host.clone();
            let tx_chunk = tx_chunk.to_vec();
            handles.push(tokio::spawn(async move {
                oneshot_chunk_send(
                    host.to_owned(),
                    port,
                    buffer_size,
                    send_timeout_us,
                    recv_timeout_us,
                    notif,
                    tx_chunk,
                    &mut result_chann_tx,
                    conn,
                )
                .await;
            }));
        }
        time::sleep(Duration::from_secs(3)).await;

        // Drop extra sender
        drop(result_chann_tx);
        let mut interval = time::interval(Duration::from_micros(period_us));

        loop {
            tokio::select! {
                _  = interval.tick() => {
                    notifier.notify_one();
                    idx += 2*conn;
                    if idx >= transactions.len() {
                        break;
                    }
                }
            }
        }

        let mut times = Vec::new();
        while let Some(v) = time::timeout(Duration::from_secs(10), rx.next())
            .await
            .unwrap_or(None)
        {
            times.push(v);
        }

        times
    }

    async fn spawn_server(
        &self,
        state: AuthorityState,
    ) -> transport::SpawnedServer<AuthorityServer> {
        let server = AuthorityServer::new(self.host.clone(), self.port, self.buffer_size, state);
        server.spawn().await.unwrap()
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
            module: ident_str!("GAS").to_owned(),
            function: ident_str!("transfer").to_owned(),
            type_arguments: Vec::new(),
            object_arguments: vec![object_ref],
            shared_object_arguments: vec![],
            pure_arguments: vec![bcs::to_bytes(&AccountAddress::from(recipient)).unwrap()],
            gas_budget: 1000,
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
                )
            } else {
                assert!(single_kinds.len() == batch_size, "Inconsistent batch size");
                TransactionData::new(
                    TransactionKind::Batch(single_kinds),
                    address,
                    gas_object_ref,
                )
            };

            let signature = Signature::new(&data, &keypair);
            let transaction = Transaction::new(data, signature);

            // Serialize transaction
            let serialized_transaction = serialize_transaction(&transaction);
            assert!(!serialized_transaction.is_empty());

            vec![
                serialized_transaction.into(),
                make_serialized_cert(keys, committee, transaction).into(),
            ]
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

async fn oneshot_chunk_send(
    host: String,
    port: u16,
    buffer_size: usize,
    send_timeout_us: u64,
    recv_timeout_us: u64,
    notif: Arc<Notify>,
    tx_chunk: Vec<Bytes>,
    result_chann_tx: &mut MpscSender<u128>,
    conn: usize,
) {
    let client = NetworkClient::new(
        host,
        port,
        buffer_size,
        Duration::from_micros(send_timeout_us),
        Duration::from_micros(recv_timeout_us),
    );
    notif.notified().await;
    let time_start = Instant::now();

    let tx_resp = client
        .batch_send(tx_chunk, conn, 0)
        .map(|x| x.unwrap())
        .concat()
        .await;

    let elapsed = time_start.elapsed().as_micros();
    result_chann_tx.send(elapsed).await.unwrap();

    let _: Vec<_> = tx_resp
        .par_iter()
        .map(|q| check_transaction_response(deserialize_message(&(q.as_ref().unwrap())[..])))
        .collect();
}
