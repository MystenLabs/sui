// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use futures::join;
use sui::benchmark::bench_types::{MicroBenchmarkResult, RemoteLoadGenConfig};

use std::panic;
use std::path::PathBuf;
use std::time::Duration;
use sui::benchmark::multi_load_generator::MultiFixedRateLoadGenerator;
use sui::benchmark::transaction_creator::TransactionCreator;
use sui::benchmark::validator_preparer::ValidatorPreparer;
use sui::config::{NetworkConfig, PersistedConfig};
use sui_types::base_types::ObjectID;
use sui_types::crypto::KeyPair;
use tokio::runtime::Builder;

#[derive(Debug, Parser)]
#[clap(
    name = "Sui Distributed Benchmark",
    about = "Benchmark of the Sui authorities on remote machines"
)]
pub struct DistributedBenchmark {
    /// Timeout for sending queries (us)
    #[clap(long, default_value = "40000000", global = true)]
    pub send_timeout_us: u64,
    /// Timeout for receiving responses (us)
    #[clap(long, default_value = "40000000", global = true)]
    pub recv_timeout_us: u64,

    /// Number of connections to the server
    #[clap(long, default_value = "0", global = true)]
    pub tcp_connections: usize,
    /// Number of database cpus
    #[clap(long, default_value = "1", global = true)]
    pub db_cpus: usize,

    /// Use Move orders
    #[clap(long, global = true)]
    pub use_native: bool,
    /// Number of chunks to send
    #[clap(long, default_value = "100")]
    pub num_chunks: usize,
    /// Size of chunks per tick
    #[clap(long, default_value = "1000")]
    pub chunk_size: usize,
    /// The time between each tick. Default 10ms
    #[clap(long, default_value = "10000")]
    pub period_us: u64,

    /// Config file for remote validators
    #[clap(long)]
    pub remote_config: PathBuf,
}

pub fn main() {
    let benchmark = DistributedBenchmark::parse();

    let remote_config: RemoteLoadGenConfig =
        PersistedConfig::read(&benchmark.remote_config).unwrap();

    let network_config: NetworkConfig =
        PersistedConfig::read(&remote_config.network_cfg_path).unwrap();

    let validator_preparer = ValidatorPreparer::new_for_remote(&network_config);
    let connections = if benchmark.tcp_connections > 0 {
        benchmark.tcp_connections
    } else {
        num_cpus::get()
    };
    let g = run_latency_microbench(
        connections,
        !benchmark.use_native,
        benchmark.num_chunks,
        benchmark.chunk_size,
        benchmark.period_us,
        remote_config.object_id_offset,
        &remote_config.account_keypair,
        network_config,
        Duration::from_micros(benchmark.send_timeout_us),
        Duration::from_micros(benchmark.recv_timeout_us),
        validator_preparer,
    );
    println!("{:?}", g);
}

fn run_latency_microbench(
    connections: usize,
    use_move: bool,
    num_chunks: usize,
    chunk_size: usize,
    period_us: u64,

    object_id_offset: ObjectID,
    sender: &KeyPair,

    network_config: NetworkConfig,

    send_timeout: Duration,
    recv_timeout: Duration,

    mut validator_preparer: ValidatorPreparer,
) -> MicroBenchmarkResult {
    // In order to simplify things, we send chunks on each connection and try to ensure all connections have equal load
    assert!(
        (num_chunks * chunk_size % connections) == 0,
        "num_transactions must {} be multiple of number of TCP connections {}",
        num_chunks * chunk_size,
        connections
    );

    // This ensures that the load generator is run at a specific object ID offset which the validators must have provisioned.
    let mut tx_cr = TransactionCreator::new_with_offset(object_id_offset);

    // These TXes are to load the network
    let load_gen_txes = tx_cr.generate_transactions(
        connections,
        use_move,
        chunk_size,
        num_chunks,
        Some(sender),
        &mut validator_preparer,
    );

    // These are probe TXes used for measuring latency
    let probe_txes = tx_cr.generate_transactions(
        1,
        use_move,
        1,
        num_chunks,
        Some(sender),
        &mut validator_preparer,
    );

    let result = panic::catch_unwind(|| {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .worker_threads(usize::min(num_cpus::get(), 24))
            .build()
            .unwrap();
        // Prep the generators
        let (mut load_gen, mut probe_gen) = runtime.block_on(async move {
            join!(
                MultiFixedRateLoadGenerator::new(
                    load_gen_txes,
                    period_us,
                    connections,
                    &network_config,
                    recv_timeout,
                    send_timeout,
                ),
                MultiFixedRateLoadGenerator::new(
                    probe_txes,
                    period_us,
                    1,
                    &network_config,
                    recv_timeout,
                    send_timeout,
                ),
            )
        });

        // Run the load gen and probes
        let (load_latencies, probe_latencies) =
            runtime.block_on(async move { join!(load_gen.start(), probe_gen.start()) });

        (load_latencies, probe_latencies)
    });

    match result {
        Ok((load_latencies, probe_latencies)) => MicroBenchmarkResult::Latency {
            load_chunk_size: chunk_size,
            load_latencies,
            tick_period_us: period_us as usize,
            chunk_latencies: probe_latencies,
        },
        Err(err) => {
            panic::resume_unwind(err);
        }
    }
}
