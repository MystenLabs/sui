// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{join, StreamExt};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{panic, thread, thread::sleep, time::Duration};
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_network::{
    network::{NetworkClient, NetworkServer},
    tonic,
};
use sui_types::{
    batch::UpdateItem,
    messages::{BatchInfoRequest, BatchInfoResponseItem},
    serialize::*,
};
use tokio::runtime::{Builder, Runtime};
use tracing::{error, info};
pub mod bench_types;
pub mod load_generator;
pub mod transaction_creator;
pub mod validator_preparer;
use crate::benchmark::{
    bench_types::{Benchmark, BenchmarkType},
    load_generator::{
        calculate_throughput, check_transaction_response, send_tx_chunks, FixedRateLoadGenerator,
    },
    transaction_creator::TransactionCreator,
    validator_preparer::ValidatorPreparer,
};

use self::bench_types::{BenchmarkResult, MicroBenchmarkResult, MicroBenchmarkType};

const FOLLOWER_BATCH_SIZE: u64 = 10_000;

pub fn run_benchmark(benchmark: Benchmark) -> BenchmarkResult {
    // Only microbenchmark is supported
    info!("benchmark : {:?}", benchmark);
    BenchmarkResult::MicroBenchmark(run_microbenchmark(benchmark))
}

fn run_microbenchmark(benchmark: Benchmark) -> MicroBenchmarkResult {
    let (host, port, type_) = match benchmark.bench_type {
        BenchmarkType::MicroBenchmark { host, port, type_ } => (host, port, type_),
    };

    let network_client = NetworkClient::new(
        host.clone(),
        port,
        benchmark.buffer_size,
        Duration::from_micros(benchmark.send_timeout_us),
        Duration::from_micros(benchmark.recv_timeout_us),
    );
    let network_server = NetworkServer::new(host, port, benchmark.buffer_size);
    let connections = if benchmark.tcp_connections > 0 {
        benchmark.tcp_connections
    } else {
        num_cpus::get()
    };
    let validator_preparer = ValidatorPreparer::new(
        benchmark.running_mode,
        benchmark.working_dir,
        benchmark.committee_size,
        network_client.base_address(),
        network_client.base_port(),
        benchmark.db_cpus,
    );
    match type_ {
        MicroBenchmarkType::Throughput { num_transactions } => run_throughout_microbench(
            network_client,
            network_server,
            connections,
            benchmark.batch_size,
            benchmark.use_move,
            num_transactions,
            validator_preparer,
        ),
        MicroBenchmarkType::Latency {
            num_chunks,
            chunk_size,
            period_us,
        } => run_latency_microbench(
            network_client,
            network_server,
            connections,
            benchmark.use_move,
            num_chunks,
            chunk_size,
            period_us,
            validator_preparer,
        ),
    }
}

fn run_throughout_microbench(
    network_client: NetworkClient,
    network_server: NetworkServer,
    connections: usize,
    batch_size: usize,
    use_move: bool,
    num_transactions: usize,
    mut validator_preparer: ValidatorPreparer,
) -> MicroBenchmarkResult {
    assert_eq!(
        num_transactions % batch_size,
        0,
        "num_transactions must integer divide batch_size",
    );
    // In order to simplify things, we send chunks on each connection and try to ensure all connections have equal load
    assert!(
        (num_transactions % connections) == 0,
        "num_transactions must be a multiple of number of TCP connections {}, got {}",
        connections,
        num_transactions,
    );
    let mut tx_cr = TransactionCreator::new();

    let chunk_size = batch_size * connections;
    let txes = tx_cr.generate_transactions(
        connections,
        use_move,
        batch_size * connections,
        num_transactions / chunk_size,
        &mut validator_preparer,
    );

    validator_preparer.deploy_validator(network_server);

    let result = panic::catch_unwind(|| {
        // Follower to observe batches
        let follower_network_client = network_client.clone();
        thread::spawn(move || {
            get_multithread_runtime()
                .block_on(async move { run_follower(follower_network_client).await });
        });

        sleep(Duration::from_secs(3));

        // Run load
        let (elapsed, resp) = get_multithread_runtime()
            .block_on(async move { send_tx_chunks(txes, network_client, connections).await });

        let _: Vec<_> = resp
            .par_iter()
            .map(|q| check_transaction_response(deserialize_message(&(q.as_ref().unwrap())[..])))
            .collect();

        elapsed
    });
    validator_preparer.clean_up();

    match result {
        Ok(elapsed) => MicroBenchmarkResult::Throughput {
            chunk_throughput: calculate_throughput(num_transactions, elapsed),
        },
        Err(err) => {
            panic::resume_unwind(err);
        }
    }
}

fn run_latency_microbench(
    network_client: NetworkClient,
    _network_server: NetworkServer,
    connections: usize,
    use_move: bool,
    num_chunks: usize,
    chunk_size: usize,
    period_us: u64,
    mut validator_preparer: ValidatorPreparer,
) -> MicroBenchmarkResult {
    // In order to simplify things, we send chunks on each connection and try to ensure all connections have equal load
    assert!(
        (num_chunks * chunk_size % connections) == 0,
        "num_transactions must {} be multiple of number of TCP connections {}",
        num_chunks * chunk_size,
        connections
    );

    let mut tx_cr = TransactionCreator::new();

    // These TXes are to load the network
    let load_gen_txes = tx_cr.generate_transactions(
        connections,
        use_move,
        chunk_size,
        num_chunks,
        &mut validator_preparer,
    );

    // These are tracer TXes used for measuring latency
    let tracer_txes =
        tx_cr.generate_transactions(1, use_move, 1, num_chunks, &mut validator_preparer);

    validator_preparer.deploy_validator(_network_server);

    let result = panic::catch_unwind(|| {
        let runtime = get_multithread_runtime();
        // Prep the generators
        let (mut load_gen, mut tracer_gen) = runtime.block_on(async move {
            join!(
                FixedRateLoadGenerator::new(
                    load_gen_txes,
                    period_us,
                    network_client.clone(),
                    connections,
                ),
                FixedRateLoadGenerator::new(tracer_txes, period_us, network_client, 1),
            )
        });

        // Run the load gen and tracers
        let (load_latencies, tracer_latencies) =
            runtime.block_on(async move { join!(load_gen.start(), tracer_gen.start()) });

        (load_latencies, tracer_latencies)
    });
    validator_preparer.clean_up();

    match result {
        Ok((load_latencies, tracer_latencies)) => MicroBenchmarkResult::Latency {
            load_chunk_size: chunk_size,
            load_latencies,
            tick_period_us: period_us as usize,
            chunk_latencies: tracer_latencies,
        },
        Err(err) => {
            panic::resume_unwind(err);
        }
    }
}

async fn run_follower(network_client: NetworkClient) {
    // We spawn a second client that listens to the batch interface
    let _batch_client_handle = tokio::task::spawn(async move {
        let uri = format!(
            "http://{}:{}",
            network_client.base_address(),
            network_client.base_port()
        )
        .parse()
        .unwrap();
        let channel = tonic::transport::Channel::builder(uri)
            .connect_timeout(network_client.send_timeout())
            .timeout(network_client.recv_timeout())
            .connect()
            .await
            .unwrap();
        let authority_client = NetworkAuthorityClient::with_channel(channel, network_client);

        let mut start = 0;

        loop {
            let receiver = authority_client
                .handle_batch_stream(BatchInfoRequest {
                    start,
                    end: start + FOLLOWER_BATCH_SIZE,
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
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((_tx_seq, _tx_digest)))) => {
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
}

fn get_multithread_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap()
}
