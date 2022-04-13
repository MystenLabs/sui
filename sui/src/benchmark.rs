// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use futures::{join, StreamExt};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::thread;
use std::{thread::sleep, time::Duration};
use sui_core::authority_client::AuthorityClient;
use sui_network::network::{NetworkClient, NetworkServer};
use sui_types::batch::UpdateItem;
use sui_types::messages::{BatchInfoRequest, BatchInfoResponseItem};
use sui_types::serialize::*;
use tokio::runtime::Builder;
use tracing::*;

pub mod bench_types;
pub mod load_generator;
pub mod transaction_creator;
use crate::benchmark::bench_types::{Benchmark, BenchmarkType};
use crate::benchmark::load_generator::{
    check_transaction_response, send_tx_chunks, spawn_authority_server, FixedRateLoadGenerator,
};
use crate::benchmark::transaction_creator::TransactionCreator;

use self::bench_types::{BenchmarkResult, MicroBenchmarkResult, MicroBenchmarkType};

pub fn run_benchmark(benchmark: Benchmark) -> BenchmarkResult {
    // Only microbenchmark support is supported
    BenchmarkResult::MicroBenchmark(run_microbenchmark(benchmark))
}

fn run_microbenchmark(benchmark: Benchmark) -> MicroBenchmarkResult {
    #[allow(irrefutable_let_patterns)]
    let (host, port, type_) =
        if let BenchmarkType::MicroBenchmark { host, port, type_ } = benchmark.bench_type {
            (host, port, type_)
        } else {
            panic!("Invalid variant")
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

    match type_ {
        MicroBenchmarkType::Throughput { num_transactions } => run_throughout_microbench(
            network_client,
            network_server,
            connections,
            benchmark.batch_size,
            benchmark.use_move,
            num_transactions,
            benchmark.committee_size,
            benchmark.db_cpus,
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
            benchmark.committee_size,
            benchmark.db_cpus,
            num_chunks,
            chunk_size,
            period_us,
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
    committee_size: usize,
    db_cpus: usize,
) -> MicroBenchmarkResult {
    assert_eq!(
        num_transactions % batch_size,
        0,
        "num_transactions must integer divide batch_size",
    );

    assert!(
        (num_transactions % connections) == 0,
        "num_transactions must {} be multiple of number of TCP connections {}",
        num_transactions,
        connections
    );
    let mut tx_cr = TransactionCreator::new(committee_size, db_cpus);

    let chunk_size = batch_size * connections;
    let txes = tx_cr.generate_transactions(
        connections,
        use_move,
        batch_size * connections,
        num_transactions / chunk_size,
    );

    // Make multi-threaded runtime for the authority
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = spawn_authority_server(network_server, tx_cr.authority_state).await;
            if let Err(e) = server.join().await {
                error!("Server ended with an error: {e}");
            }
        });
    });

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();

    // Wait for server start
    sleep(Duration::from_secs(3));

    // Follower to observe batches
    let follower_network_client = network_client.clone();
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move { run_follower(follower_network_client).await });
    });

    sleep(Duration::from_secs(3));

    // Run load
    let (elapsed, resp) =
        runtime.block_on(async move { send_tx_chunks(txes, network_client, connections).await });

    let _: Vec<_> = resp
        .par_iter()
        .map(|q| check_transaction_response(deserialize_message(&(q.as_ref().unwrap())[..])))
        .collect();
    MicroBenchmarkResult::Throughput {
        chunk_throughput: 1_000_000.0 * num_transactions as f64 / elapsed as f64,
    }
}

fn run_latency_microbench(
    network_client: NetworkClient,
    network_server: NetworkServer,
    connections: usize,
    use_move: bool,
    committee_size: usize,
    db_cpus: usize,

    num_chunks: usize,
    chunk_size: usize,
    period_us: u64,
) -> MicroBenchmarkResult {
    assert!(
        (num_chunks * chunk_size % connections) == 0,
        "num_transactions must {} be multiple of number of TCP connections {}",
        num_chunks * chunk_size,
        connections
    );
    let mut tx_cr = TransactionCreator::new(committee_size, db_cpus);

    // These TXes are to load the network
    let load_gen_txes = tx_cr.generate_transactions(connections, use_move, chunk_size, num_chunks);

    // These are tracer TXes used for measuring latency
    let tracer_txes = tx_cr.generate_transactions(1, use_move, 1, num_chunks);

    // Make multi-threaded runtime for the authority
    thread::spawn(move || {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = spawn_authority_server(network_server, tx_cr.authority_state).await;
            if let Err(e) = server.join().await {
                error!("Server ended with an error: {e}");
            }
        });
    });

    // Wait for server start
    sleep(Duration::from_secs(3));

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();

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

    MicroBenchmarkResult::Latency {
        load_chunk_size: chunk_size,
        load_latencies,
        tick_period_us: period_us as usize,
        chunk_latencies: tracer_latencies,
    }
}

async fn run_follower(network_client: NetworkClient) {
    // We spawn a second client that listens to the batch interface
    let _batch_client_handle = tokio::task::spawn(async move {
        let authority_client = AuthorityClient::new(network_client);

        let mut start = 0;

        loop {
            let receiver = authority_client
                .handle_batch_streaming_as_stream(BatchInfoRequest {
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
