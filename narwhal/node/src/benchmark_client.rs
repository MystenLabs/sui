// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use clap::*;
use eyre::Context;
use futures::future::join_all;
use narwhal_node::metrics::NarwhalBenchMetrics;
use prometheus::Registry;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::{
    net::TcpStream,
    time::{interval, sleep, Duration, Instant},
};
use tracing::{info, subscriber::set_global_default, warn};
use tracing_subscriber::filter::EnvFilter;
use types::{TransactionProto, TransactionsClient};
use url::Url;

/// Benchmark client for Narwhal and Tusk
///
/// To run the benchmark client following are required:
/// * the size of the transactions via the --size property
/// * the worker address <ADDR> to send the transactions to. A url format is expected ex http://127.0.0.1:7000
/// * the rate of sending transactions via the --rate parameter
/// Optionally the --nodes parameter can be passed where a list of worker addresses
/// should be passed. The benchmarking client will first try to connect to all of those nodes before start sending
/// any transactions. That confirms the system is up and running and ready to start processing the transactions.
#[derive(Parser)]
#[clap(name = "Narwhal Stress Testing Framework")]
struct App {
    /// The network address of the node where to send txs. A url format is expected ex 'http://127.0.0.1:7000'
    #[clap(long, value_parser = parse_url, global = true)]
    addr: Url,
    /// The size of each transaciton in bytes
    #[clap(long, default_value = "512", global = true)]
    size: usize,
    /// The rate (txs/s) at which to send the transactions
    #[clap(long, default_value = "100", global = true)]
    rate: u64,
    /// Network addresses that must be reachable before starting the benchmark.
    #[clap(long, value_delimiter = ',', value_parser = parse_url, global = true)]
    nodes: Vec<Url>,
    /// Optional duration of the benchmark in seconds. If not provided the benchmark will run forever.
    #[clap(long, global = true)]
    duration: Option<u64>,
    #[clap(long, default_value = "0.0.0.0", global = true)]
    client_metric_host: String,
    #[clap(long, default_value = "8081", global = true)]
    client_metric_port: u16,
}

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    let app = App::parse();

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    cfg_if::cfg_if! {
        if #[cfg(feature = "benchmark")] {
            let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
            let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
                                     .with_env_filter(env_filter)
                                     .with_timer(timer).with_ansi(false);
        } else {
            let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
        }
    }
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();

    set_global_default(subscriber).expect("Failed to set subscriber");

    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", app.client_metric_host, app.client_metric_port)
            .parse()
            .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let metrics = NarwhalBenchMetrics::new(&registry);

    let target = app.addr;
    let size = app.size;
    let rate = app.rate;
    let nodes = app.nodes;

    let duration: Option<Duration> = match app.duration {
        Some(d) => {
            info!("Benchmark Duration: {d}");
            Some(Duration::from_secs(d))
        }
        None => None,
    };

    info!("Node address: {target}");

    // NOTE: This log entry is used to compute performance.
    info!("Transactions size: {size} B");

    // NOTE: This log entry is used to compute performance.
    info!("Transactions rate: {rate} tx/s");

    let client = Client {
        target,
        size,
        rate,
        nodes,
        duration,
        metrics,
    };

    // Wait for all nodes to be online and synchronized, if any.
    client.wait().await;

    // Start the benchmark.
    client.send().await.context("Failed to submit transactions")
}

struct Client {
    target: Url,
    size: usize,
    rate: u64,
    nodes: Vec<Url>,
    duration: Option<Duration>,
    metrics: NarwhalBenchMetrics,
}

impl Client {
    pub async fn send(&self) -> Result<(), eyre::Report> {
        // The transaction size must be at least 100 bytes to ensure all txs are different.
        if self.size < 100 {
            return Err(eyre::Report::msg(
                "Transaction size must be at least 100 bytes",
            ));
        }

        let mut handles = Vec::new();

        // TODO: figure out how to scale the client without needing to scale tasks
        // Current results are showing about 10 tx/s per task.
        let num_parallel_tasks = self.rate.min(25000);
        let rate_per_task = self.rate / num_parallel_tasks;
        let target_tx_interval: Duration = Duration::from_millis(1000 / rate_per_task);
        info!(
            "Distributing transactions across {num_parallel_tasks} parallel tasks, with  \
            each task sending approximately {rate_per_task} transactions. Each task  \
            sends 1 transaction every {target_tx_interval:#?} to achieve a rate of {} tx/sec",
            self.rate
        );

        let start_time = Instant::now();
        let metrics = Arc::new(self.metrics.clone());

        for i in 0..num_parallel_tasks {
            // Connect to the mempool.
            let mut client = TransactionsClient::connect(self.target.as_str().to_owned())
                .await
                .context(format!("failed to connect to {}", self.target))?;
            let metrics_clone = metrics.clone();
            let task_id = i;
            let client_id = self.target.port().unwrap() as u64;
            let size = self.size;

            let handle = tokio::spawn(async move {
                let interval = interval(target_tx_interval);
                tokio::pin!(interval);
                let mut rng = StdRng::seed_from_u64(client_id);
                let mut random: u64 = rng.gen(); // 8 bytes
                let mut counter = 0;

                loop {
                    interval.as_mut().tick().await;

                    let timestamp = (timestamp_utc().as_millis() as u64).to_le_bytes();
                    counter += 1;
                    random += counter * task_id;

                    let mut transaction = vec![0u8; size];
                    transaction[0..8].copy_from_slice(&client_id.to_le_bytes()); // 8 bytes
                    transaction[8..16].copy_from_slice(&timestamp); // 8 bytes
                    transaction[16..24].copy_from_slice(&random.to_le_bytes()); // 8 bytes

                    let tx_proto = TransactionProto {
                        transaction: Bytes::from(transaction),
                    };

                    let now = Instant::now();

                    metrics_clone.narwhal_client_num_submitted.inc();

                    if let Err(e) = client.submit_transaction(tx_proto).await {
                        warn!("Failed to send transaction: {e}");
                        metrics_clone.narwhal_client_num_error.inc();
                    } else {
                        metrics_clone.narwhal_client_num_success.inc();
                        // TODO: properly compute the latency from submission to consensus output and successful commits
                        // record client latencies per transaction
                        let latency_s = now.elapsed().as_secs_f64();
                        let latency_squared_s = latency_s.powf(2.0);
                        metrics_clone.narwhal_client_latency_s.observe(latency_s);
                        metrics_clone
                            .narwhal_client_latency_squared_s
                            .inc_by(latency_squared_s);
                    }
                }
            });
            handles.push(handle);
        }

        let monitoring_interval = Duration::from_secs(1);

        let metrics_clone = metrics.clone();
        let end_time = self.duration.map(|d| Instant::now() + d);

        // Spawn a monitoring task
        let monitor_handle = tokio::spawn(async move {
            let monitor_interval = interval(monitoring_interval);
            tokio::pin!(monitor_interval);

            loop {
                monitor_interval.as_mut().tick().await;

                if let Some(end) = end_time {
                    if Instant::now() > end {
                        break;
                    }
                }

                let time_from_start = start_time.elapsed();
                metrics_clone
                    .narwhal_benchmark_duration
                    .set(time_from_start.as_secs() as i64);

                // Log the metrics
                let benchmark_duration = metrics_clone.narwhal_benchmark_duration.get();
                let total_submitted = metrics_clone.narwhal_client_num_submitted.get();
                let total_success = metrics_clone.narwhal_client_num_success.get();
                let total_error = metrics_clone.narwhal_client_num_error.get();
                info!(
                    "{}s Elapsed, Total Submitted: {}, Total Success: {}, Total Error: {}, Rate {} tx/sec",
                    benchmark_duration,
                    total_submitted,
                    total_success,
                    total_error,
                    total_submitted / time_from_start.as_secs().max(1)
                );
            }
        });

        tokio::select! {
            _ = monitor_handle => {
                info!("Monitoring task completed. Ending benchmark.");
            }
            _ = join_all(handles) => {
                info!("All transaction-sending tasks completed.");
            }
        }

        Ok(())
    }

    pub async fn wait(&self) {
        // Wait for all nodes to be online.
        let mut all_nodes = self.nodes.clone();
        all_nodes.push(self.target.clone());
        join_all(all_nodes.iter().cloned().map(|address| {
            info!("Waiting for {address} to be online...");
            tokio::spawn(async move {
                while TcpStream::connect(&*address.socket_addrs(|| None).unwrap())
                    .await
                    .is_err()
                {
                    sleep(Duration::from_millis(10)).await;
                }
            })
        }))
        .await;
    }
}

fn parse_url(s: &str) -> Result<Url, url::ParseError> {
    Url::from_str(s)
}

pub fn timestamp_utc() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
}
