// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use clap::*;
use eyre::Context;
use futures::future::join_all;
use mysten_network::Multiaddr;
use narwhal_node::metrics::NarwhalBenchMetrics;
use prometheus::Registry;
use rand::{
    rngs::{SmallRng, StdRng},
    Rng, RngCore, SeedableRng,
};
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
use worker::LazyNarwhalClient;

/// Benchmark client for Narwhal and Tusk
///
/// To run the benchmark client following are required:
/// * the size of the transactions via the --size property
/// * the worker address <ADDR> to send the transactions to. A url format is expected ex http://127.0.0.1:7000
/// * the rate of sending transactions via the --rate parameter
///
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
    // Local or remote client operating mode.
    #[clap(long, default_value = "remote", value_parser)]
    operating_mode: OperatingMode,
}

#[derive(Clone)]
pub enum OperatingMode {
    // Submit transactions via local channel
    Local,
    // Submit transactions via grpc
    Remote,
}

impl std::str::FromStr for OperatingMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(OperatingMode::Local),
            "remote" => Ok(OperatingMode::Remote),
            _ => Err("must be 'local' or 'remote'".to_string()),
        }
    }
}

#[allow(dead_code)]
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
    let operating_mode = app.operating_mode;

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
        target: target.clone(),
        size,
        rate,
        nodes,
        duration,
        metrics,
        local_client: Arc::new(LazyNarwhalClient::new(url_to_multiaddr(&target)?)),
        operating_mode,
    };

    // Wait for all nodes to be online and synchronized, if any.
    client.wait().await;

    // Start the benchmark.
    client.send().await.context("Failed to submit transactions")
}

pub struct Client {
    pub target: Url,
    pub size: usize,
    pub rate: u64,
    pub nodes: Vec<Url>,
    pub duration: Option<Duration>,
    pub metrics: NarwhalBenchMetrics,
    pub local_client: Arc<LazyNarwhalClient>,
    pub operating_mode: OperatingMode,
}

impl Client {
    pub async fn start(&self) -> Result<(), eyre::Report> {
        self.wait().await;
        self.send().await
    }

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
        let base_rate_per_task = self.rate / num_parallel_tasks;
        let remaining_transactions = self.rate % num_parallel_tasks;
        let base_target_tx_interval: Duration = Duration::from_millis(1000 / base_rate_per_task);
        info!(
            "Distributing transactions across {num_parallel_tasks} parallel tasks, with  \
            each task sending approximately {base_rate_per_task} transactions. Each task  \
            sends 1 transaction every {base_target_tx_interval:#?} to achieve a rate of {} tx/sec",
            self.rate
        );

        let start_time = Instant::now();
        let metrics = Arc::new(self.metrics.clone());

        for i in 0..num_parallel_tasks {
            let task_rate = if i < remaining_transactions {
                base_rate_per_task + 1
            } else {
                base_rate_per_task
            };
            let task_interval = Duration::from_millis(1000 / task_rate);

            let local_client_clone = Arc::clone(&self.local_client);
            let mut grpc_client = TransactionsClient::connect(self.target.as_str().to_owned())
                .await
                .context(format!("failed to connect to {}", self.target))?;
            let metrics_clone = metrics.clone();
            let task_id = i;
            let client_id = self.target.port().unwrap() as u64;
            let size = self.size;
            let operating_mode = self.operating_mode.clone();

            let handle = tokio::spawn(async move {
                let interval = interval(task_interval);
                tokio::pin!(interval);
                let mut rng = StdRng::seed_from_u64(client_id);
                let mut random: u64 = rng.gen(); // 8 bytes
                let mut counter = 0;

                let local_client = match operating_mode {
                    OperatingMode::Local => Some(local_client_clone.get().await),
                    OperatingMode::Remote => None,
                };

                loop {
                    interval.as_mut().tick().await;

                    let timestamp = (timestamp_utc().as_millis() as u64).to_le_bytes();
                    counter += 1;
                    random += counter * task_id;

                    let mut transaction = vec![0u8; size];

                    let mut fast_rng = SmallRng::from_entropy();
                    fast_rng.fill_bytes(&mut transaction);

                    transaction[0..8].copy_from_slice(&client_id.to_le_bytes()); // 8 bytes
                    transaction[8..16].copy_from_slice(&timestamp); // 8 bytes
                    transaction[16..24].copy_from_slice(&random.to_le_bytes()); // 8 bytes

                    let submission_error: Option<eyre::Report>;
                    if local_client.is_some() {
                        if let Err(e) = submit_to_consensus(&local_client_clone, transaction).await
                        {
                            submission_error = Some(e)
                        } else {
                            submission_error = None;
                        }
                    } else {
                        let tx_proto = TransactionProto {
                            transactions: vec![Bytes::from(transaction)],
                        };
                        if let Err(e) = grpc_client.submit_transaction(tx_proto).await {
                            submission_error = Some(eyre::Report::msg(format!("{e}")));
                        } else {
                            submission_error = None;
                        }
                    }

                    let now = Instant::now();

                    metrics_clone.narwhal_client_num_submitted.inc();

                    if let Some(submission_error) = submission_error {
                        warn!("Failed to send transaction: {submission_error}");
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

pub fn parse_url(s: &str) -> Result<Url, url::ParseError> {
    Url::from_str(s)
}

pub fn url_to_multiaddr(url: &Url) -> Result<Multiaddr, eyre::Report> {
    let host_str = url
        .host_str()
        .ok_or(eyre::Report::msg("URL does not have a host"))?;
    let port = url
        .port()
        .ok_or(eyre::Report::msg("URL does not specify a port"))?;

    Multiaddr::try_from(format!("/ip4/{}/tcp/{}/http", host_str, port))
        .map_err(|_| eyre::Report::msg("Failed to create Multiaddr from URL"))
}

pub fn timestamp_utc() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
}

async fn submit_to_consensus(
    client_arc: &Arc<LazyNarwhalClient>,
    transaction: Vec<u8>,
) -> Result<(), eyre::Report> {
    let client = {
        let c = client_arc.client.load();
        if c.is_some() {
            c
        } else {
            client_arc.client.store(Some(client_arc.get().await));
            client_arc.client.load()
        }
    };
    let client = client.as_ref().unwrap().load();
    client
        .submit_transactions(vec![transaction])
        .await
        .map_err(|e| eyre::Report::msg(format!("Failed to submit to consensus: {:?}", e)))?;
    Ok(())
}
