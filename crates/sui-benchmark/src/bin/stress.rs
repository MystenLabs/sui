// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use clap::*;

use prometheus::Registry;
use rand::seq::SliceRandom;
use rand::Rng;
use tokio::time::sleep;

use std::sync::Arc;
use std::time::Duration;
use sui_benchmark::drivers::bench_driver::BenchDriver;
use sui_benchmark::drivers::driver::Driver;
use sui_benchmark::drivers::BenchmarkCmp;
use sui_benchmark::drivers::BenchmarkStats;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};

use sui_node::metrics;

use sui_benchmark::benchmark_setup::Env;
use sui_benchmark::options::Opts;

use sui_benchmark::workloads::workload_configuration::WorkloadConfiguration;

use sui_benchmark::system_state_observer::SystemStateObserver;
use tokio::runtime::Builder;
use tokio::sync::Barrier;

/// To spin up a local cluster and direct some load
/// at it with 50/50 shared and owned traffic, use
/// it something like:
/// ```cargo run  --release  --package sui-benchmark
/// --bin stress -- --num-client-threads 12 \
/// --num-server-threads 10 \
/// --num-transfer-accounts 2 \
/// bench \
/// --target-qps 100 \
/// --in-flight-ratio 2 \
/// --shared-counter 50 \
/// --transfer-object 50```
/// To point the traffic to an already running cluster,
/// use it something like:
/// ```cargo run  --release  --package sui-benchmark --bin stress -- --num-client-threads 12 \
/// --num-server-threads 10 \
/// --num-transfer-accounts 2 \
/// --primary-gas-id 0x59931dcac57ba20d75321acaf55e8eb5a2c47e9f \
/// --genesis-blob-path /tmp/genesis.blob \
/// --keystore-path /tmp/sui.keystore bench \
/// --target-qps 100 \
/// --in-flight-ratio 2 \
/// --shared-counter 50 \
/// --transfer-object 50```
#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    // TODO: query the network for the current protocol version.
    let protocol_config = match opts.protocol_version {
        Some(v) => ProtocolConfig::get_for_version(ProtocolVersion::new(v)),
        None => ProtocolConfig::get_for_max_version(),
    };

    let max_num_new_move_object_ids = protocol_config.max_num_new_move_object_ids();
    let max_num_transferred_move_object_ids = protocol_config.max_num_transferred_move_object_ids();

    if (opts.gas_request_chunk_size > max_num_new_move_object_ids)
        || (opts.gas_request_chunk_size > max_num_transferred_move_object_ids)
    {
        eprintln!(
            "`gas-request-chunk-size` must be less than the maximum number of new IDs {max_num_new_move_object_ids} and the maximum number of transferred IDs {max_num_transferred_move_object_ids}",
        );
    }

    let mut config = telemetry_subscribers::TelemetryConfig::new();
    config.log_string = Some("warn".to_string());
    if !opts.log_path.is_empty() {
        config.log_file = Some(opts.log_path.clone());
    }
    let _guard = config.with_env().init();

    let registry_service = metrics::start_prometheus_server(
        format!("{}:{}", opts.client_metric_host, opts.client_metric_port)
            .parse()
            .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);

    let barrier = Arc::new(Barrier::new(2));
    let cloned_barrier = barrier.clone();
    let env = if opts.local { Env::Local } else { Env::Remote };
    let bench_setup = env.setup(cloned_barrier, &registry, &opts).await?;
    let system_state_observer = {
        // Only need to get system state from one proxy as it is shared for the
        // whole network.
        let mut system_state_observer = SystemStateObserver::new(
            bench_setup
                .proxies
                .choose(&mut rand::thread_rng())
                .context("Failed to get proxy for system state observer")?
                .clone(),
        );
        system_state_observer.state.changed().await?;
        eprintln!(
            "Found new state (reference gas price and/or protocol config) from system state object = {:?}",
            system_state_observer.state.borrow().reference_gas_price
        );
        Arc::new(system_state_observer)
    };
    let stress_stat_collection = opts.stress_stat_collection;
    barrier.wait().await;

    // Add a small randomized delay before workloads start, to even out the traffic.
    const START_DELAY_INTERVAL: Duration = Duration::from_secs(2);
    const START_DELAY_MAX_JITTER_MS: u64 = 2000;
    if opts.staggered_start_max_multiplier > 0 {
        let delay = START_DELAY_INTERVAL
            * rand::thread_rng().gen_range(0..opts.staggered_start_max_multiplier)
            + Duration::from_millis(rand::thread_rng().gen_range(0..START_DELAY_MAX_JITTER_MS));
        sleep(delay).await;
    }

    // create client runtime
    let client_runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(opts.num_client_threads as usize)
        .build()
        .unwrap();
    let prev_benchmark_stats_path = opts.compare_with.clone();
    let curr_benchmark_stats_path = opts.benchmark_stats_path.clone();
    let registry_clone = registry.clone();
    let handle = std::thread::spawn(move || {
        client_runtime.block_on(async move {
            let workloads = WorkloadConfiguration::configure(
                bench_setup.bank,
                &opts,
                system_state_observer.clone(),
            )
            .await?;
            let interval = opts.run_duration;
            // We only show continuous progress in stderr
            // if benchmark is running in unbounded mode,
            // otherwise summarized benchmark results are
            // published in the end
            let show_progress = interval.is_unbounded();
            let driver = BenchDriver::new(opts.stat_collection_interval, stress_stat_collection);
            driver
                .run(
                    bench_setup.proxies,
                    workloads,
                    system_state_observer,
                    &registry_clone,
                    show_progress,
                    interval,
                )
                .await
        })
    });
    let joined = handle.join();
    if let Err(err) = joined {
        Err(anyhow!("Failed to join client runtime: {:?}", err))
    } else {
        // send signal to stop the server runtime
        bench_setup
            .shutdown_notifier
            .send(())
            .expect("Failed to stop server runtime");
        bench_setup
            .server_handle
            .join()
            .expect("Failed to join the server handle");
        match joined {
            Ok(result) => match result {
                Ok((benchmark_stats, stress_stats)) => {
                    let benchmark_table = benchmark_stats.to_table();
                    eprintln!("Benchmark Report:");
                    eprintln!("{}", benchmark_table);

                    if stress_stat_collection {
                        eprintln!("Stress Performance Report:");
                        let stress_stats_table = stress_stats.to_table();
                        eprintln!("{}", stress_stats_table);
                    }

                    if !prev_benchmark_stats_path.is_empty() {
                        let data = std::fs::read_to_string(&prev_benchmark_stats_path)?;
                        let prev_stats: BenchmarkStats = serde_json::from_str(&data)?;
                        let cmp = BenchmarkCmp {
                            new: &benchmark_stats,
                            old: &prev_stats,
                        };
                        let cmp_table = cmp.to_table();
                        eprintln!(
                            "Benchmark Comparison Report[{}]:",
                            prev_benchmark_stats_path
                        );
                        eprintln!("{}", cmp_table);
                    }
                    if !curr_benchmark_stats_path.is_empty() {
                        let serialized = serde_json::to_string(&benchmark_stats)?;
                        std::fs::write(curr_benchmark_stats_path, serialized)?;
                    }
                    let num_error_txes = benchmark_stats.num_error_txes;
                    if num_error_txes > 0 {
                        return Err(anyhow!("{} transactions ended in an error", num_error_txes));
                    }
                }
                Err(e) => return Err(anyhow!("{e}")),
            },
            Err(e) => return Err(anyhow!("{e:?}")),
        }
        Ok(())
    }
}
