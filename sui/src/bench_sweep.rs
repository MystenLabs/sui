// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use bytes::Bytes;
use futures::stream::StreamExt;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use rayon::prelude::*;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use sui::config::PORT_ALLOCATOR;
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
use std::sync::Arc;
use std::thread;

const DB_CPUS: i32 = 8;
const RECV_TIMEOUT_US: Duration = Duration::from_micros(40000000);
const SEND_TIMEOUT_US: Duration = Duration::from_micros(40000000);
const BUFFER_SIZE: usize = transport::DEFAULT_MAX_DATAGRAM_SIZE;
const COMMITEE_SIZE: u64 = 4;

// This is to mitigate the noise. We consider two samples within DIFFERENCE_TOLERANCE
// of each  other to be the same
const DIFFERENCE_TOLERANCE: u64 = 5_000u64;
// This is the number of samples below the max seen after which we will consider that the peak is reached
const NUMBER_OF_LOWER_DEVIATIONS: u64 = 10;

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "Sui Benchmark",
    about = "Local end-to-end test and benchmark of the Sui protocol"
)]
struct ThroughoutSweepCommands {
    /// Hostname
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,

    /// Number of transactions to start with
    #[structopt(long, default_value = "10000")]
    tx_start: usize,

    /// Number of transactions to end at (inclusive)
    #[structopt(long, default_value = "200000")]
    tx_end: usize,

    /// Number of transactions to step by
    #[structopt(long, default_value = "10000")]
    tx_step: usize,

    /// Use Move orders
    #[structopt(long)]
    use_move: bool,

    /// Size of the batch
    #[structopt(long, default_value = "1000")]
    batch_size: usize,
}

fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");

    let cmd = ThroughoutSweepCommands::from_args();

    // This number was chosen from experimentation but should ideally be varied during benchmarking
    let batch_size = cmd.batch_size;
    let host = cmd.host.as_str();
    let use_move = cmd.use_move;
    let tx_start = cmd.tx_start;
    let tx_step = cmd.tx_step;
    let tx_end = cmd.tx_end;
    let connections = num_cpus::get();

    let r1 = benchmark_txes(
        batch_size,
        host.to_owned(),
        use_move,
        connections,
        tx_start,
        tx_end,
        tx_step,
        false,
    );

    println!("{:?}", r1);
}

fn benchmark_txes(
    batch_size: usize,
    host: String,
    use_move: bool,
    connections: usize,
    tx_start: usize,
    tx_end: usize,
    tx_step: usize,
    stop_early_at_max: bool,
) -> Vec<(usize, u64, u128)> {
    let mut results = vec![];
    let mut max = 0u64;
    // Number of times consecutive times we can dip below max before stopping
    let mut num_low_samples = 0;

    for num_tx in (tx_start..tx_end + 1).step_by(tx_step) {
        if stop_early_at_max && num_low_samples >= NUMBER_OF_LOWER_DEVIATIONS {
            break;
        }
        info!("Running for num txes: {}", num_tx);
        // To avoid thread conflicts
        let port = PORT_ALLOCATOR
            .lock()
            .unwrap()
            .next_port()
            .expect("No available port.");

        let (throughout, latency_us) = single_authority_throughput(
            num_tx,
            batch_size,
            host.to_string(),
            port,
            connections,
            use_move,
        );

        // From experimentation, the plot is concave.
        // Ideally we should be able to find the peak more optimally by binary search, but the results are noisy
        if throughout < max - DIFFERENCE_TOLERANCE {
            num_low_samples += 1;
        } else {
            max = std::cmp::max(max, throughout);
            num_low_samples = 0;
        }
        results.push((num_tx, throughout, latency_us));
        thread::sleep(Duration::from_millis(10));
    }
    results
}

fn single_authority_throughput(
    num_tx: usize,
    batch_size: usize,
    host: String,
    port: u16,
    connections: usize,
    use_move: bool,
) -> (u64, u128) {
    assert_eq!(
        num_tx % batch_size,
        0,
        "num_transactions must integer divide batch_size",
    );

    let (state, transactions) = make_structures(num_tx, batch_size, COMMITEE_SIZE, use_move);

    assert!(
        (num_tx * 2 / connections) % 2 == 0,
        "Each tx and their cert must be sent in order. Multiple TCP connections will break requests into chunks, and chunk size must be an even number so it doesn't break each tx and cert pair"
    );
    let h = host.clone();
    // TODO: kill thread at end of function
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = spawn_server(state, host.clone(), port).await;
            if let Err(err) = server.join().await {
                error!("Server ended with an error: {}", err);
            }
        });
    });

    // Make a single-core runtime for the client.
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();

    // Launch in batch mode
    let ret = runtime.block_on(batch_execute(
        h,
        port,
        connections,
        batch_size,
        transactions,
    ));
    (ret.0 as u64, ret.1)
}

fn make_structures(
    num_tx: usize,
    batch_size: usize,
    committee_size: u64,
    use_move: bool,
) -> (AuthorityState, Vec<Bytes>) {
    let mut keys = Vec::new();
    for _ in 0..committee_size {
        let (_, key_pair) = get_key_pair();
        let name = *key_pair.public_key_bytes();
        keys.push((name, key_pair));
    }
    let committee = Committee::new(keys.iter().map(|(k, _)| (*k, 1)).collect());

    // Pick an authority and create state.
    let (public_auth0, secret_auth0) = keys.pop().unwrap();

    // Create a random directory to store the DB
    let path = env::temp_dir().join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();
    info!("Open database on path: {:?}", path.as_os_str());

    let mut opts = Options::default();
    opts.increase_parallelism(DB_CPUS);
    opts.set_write_buffer_size(256 * 1024 * 1024);
    opts.enable_statistics();
    opts.set_stats_dump_period_sec(5);
    opts.set_enable_pipelined_write(true);

    // NOTE: turn off the WAL, but is not guaranteed to
    // recover from a crash. Keep turned off to max safety,
    // but keep as an option if we periodically flush WAL
    // manually.
    // opts.set_manual_wal_flush(true);

    let store = Arc::new(AuthorityStore::open(path, Some(opts)));
    let store_bis = store.clone();

    // Seed user accounts.
    let rt = Runtime::new().unwrap();

    info!("Init Authority.");
    let state = rt.block_on(async {
        AuthorityState::new(
            committee.clone(),
            public_auth0,
            Arc::pin(secret_auth0),
            store,
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await
    });

    info!("Generate empty store with Genesis.");
    let (address, keypair) = get_key_pair();
    // Lets not collide with genesis objects.
    let offset = 10000;
    let tx_count = num_tx / batch_size;

    let account_gas_objects: Vec<_> = (0..tx_count)
        .into_par_iter()
        .map(|x| {
            let mut objects = vec![];
            for i in 0..batch_size {
                let mut obj_id = [0; 20];
                obj_id[..8].clone_from_slice(&(offset + x * batch_size + i).to_be_bytes()[..8]);
                let object_id: ObjectID = ObjectID::from(obj_id);
                let object = if use_move {
                    Object::with_id_owner_gas_coin_object_for_testing(
                        object_id,
                        SequenceNumber::new(),
                        address,
                        1,
                    )
                } else {
                    Object::with_id_owner_for_testing(object_id, address)
                };
                objects.push(object);
            }

            let mut gas_object_id = [0; 20];
            gas_object_id[8..16].clone_from_slice(&(offset + x).to_be_bytes()[..8]);
            let gas_object_id = ObjectID::from(gas_object_id);
            let gas_object = Object::with_id_owner_for_testing(gas_object_id, address);
            assert!(gas_object.version() == SequenceNumber::from(0));

            (objects, gas_object)
        })
        .collect();

    // Bulk load objects
    let all_objects: Vec<_> = account_gas_objects
        .iter()
        .flat_map(|(objects, gas)| objects.iter().chain(std::iter::once(gas)))
        .collect();
    assert_eq!(
        all_objects
            .iter()
            .map(|o| o.id())
            .collect::<HashSet<_>>()
            .len(),
        tx_count * (batch_size + 1)
    );
    store_bis.bulk_object_insert(&all_objects[..]).unwrap();

    // Make one transaction per account (transfer transaction + confirmation).
    let transactions: Vec<_> = account_gas_objects
        .par_iter()
        .map(|(objects, gas_obj)| {
            let next_recipient: SuiAddress = get_key_pair().0;
            let mut single_kinds = vec![];
            for object in objects {
                let object_ref = object.compute_object_reference();

                let kind = if use_move {
                    // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
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
                        pure_arguments: vec![
                            bcs::to_bytes(&AccountAddress::from(next_recipient)).unwrap()
                        ],
                        gas_budget: 1000,
                    })
                } else {
                    SingleTransactionKind::Transfer(Transfer {
                        recipient: next_recipient,
                        object_ref,
                    })
                };
                single_kinds.push(kind);
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

            let mut transactions = vec![serialized_transaction.into()];

            // Make certificate
            let mut certificate = CertifiedTransaction::new(transaction);
            for i in 0..committee.quorum_threshold() {
                let (pubx, secx) = keys.get(i).unwrap();
                let sig = AuthoritySignature::new(&certificate.transaction.data, secx);
                certificate.signatures.push((*pubx, sig));
            }

            let serialized_certificate = serialize_cert(&certificate);
            assert!(!serialized_certificate.is_empty());

            transactions.push(serialized_certificate.into());

            transactions
        })
        .flatten()
        .collect();

    (state, transactions)
}

async fn spawn_server(state: AuthorityState, host: String, port: u16) -> transport::SpawnedServer {
    let server = AuthorityServer::new(host, port, BUFFER_SIZE, state);
    server.spawn().await.unwrap()
}

async fn batch_execute(
    host: String,
    port: u16,
    connections: usize,
    batch_size: usize,
    transactions: Vec<Bytes>,
) -> (f64, u128) {
    // Give the server time to be ready
    time::sleep(Duration::from_millis(1000)).await;
    let items_number = transactions.len() / 2;

    info!("Number of TCP connections: {}", connections);
    let mass_client = NetworkClient::new(
        host.clone(),
        port,
        BUFFER_SIZE,
        SEND_TIMEOUT_US,
        RECV_TIMEOUT_US,
    );

    let time_start = Instant::now();
    let responses = mass_client
        .batch_send(transactions, connections, 0)
        .map(|x| x.unwrap())
        .concat()
        .await;
    let elapsed_time = time_start.elapsed().as_micros();

    // Check the responses for errors
    for resp in &responses {
        let reply_message = deserialize_message(&(resp.as_ref().unwrap())[..]);
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
            _ => (),
        };
    }
    (
        1_000_000.0 * (items_number as f64 * batch_size as f64) / (elapsed_time as f64),
        elapsed_time,
    )
}
