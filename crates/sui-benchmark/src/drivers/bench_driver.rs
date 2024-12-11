// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::try_join_all;
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use prometheus::register_histogram_vec_with_registry;
use prometheus::IntCounterVec;
use prometheus::Registry;
use prometheus::{register_counter_vec_with_registry, register_gauge_vec_with_registry};
use prometheus::{register_int_counter_vec_with_registry, CounterVec};
use prometheus::{register_int_gauge_with_registry, GaugeVec};
use prometheus::{HistogramVec, IntGauge};
use rand::seq::SliceRandom;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use crate::drivers::driver::Driver;
use crate::drivers::HistogramWrapper;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::ExpectedFailureType;
use crate::workloads::{GroupID, WorkloadInfo};
use crate::{ExecutionEffects, ValidatorProxy};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::Committee;
use sui_types::quorum_driver_types::QuorumDriverError;
use sui_types::transaction::{Transaction, TransactionDataAPI};
use sysinfo::{CpuExt, System, SystemExt};
use tokio::sync::Barrier;
use tokio::task::{JoinHandle, JoinSet};
use tokio::{time, time::Instant};
use tracing::{debug, error, info, warn};

use super::Interval;
use super::{BenchmarkStats, StressStats};
pub struct BenchMetrics {
    pub benchmark_duration: IntGauge,
    pub num_success: IntCounterVec,
    pub num_error: IntCounterVec,
    pub num_expected_error: IntCounterVec,
    pub num_submitted: IntCounterVec,
    pub num_in_flight: GaugeVec,
    pub latency_s: HistogramVec,
    pub latency_squared_s: CounterVec,
    pub validators_in_tx_cert: IntCounterVec,
    pub validators_in_effects_cert: IntCounterVec,
    pub cpu_usage: GaugeVec,
    pub num_success_cmds: IntCounterVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl BenchMetrics {
    fn new(registry: &Registry) -> Self {
        BenchMetrics {
            benchmark_duration: register_int_gauge_with_registry!(
                "benchmark_duration",
                "Duration of the benchmark",
                registry,
            )
            .unwrap(),
            num_success: register_int_counter_vec_with_registry!(
                "num_success",
                "Total number of transaction success",
                &["workload"],
                registry,
            )
            .unwrap(),
            num_expected_error: register_int_counter_vec_with_registry!(
                "num_expected_error",
                "Total number of transaction errors that were expected",
                &["workload"],
                registry,
            )
            .unwrap(),
            num_success_cmds: register_int_counter_vec_with_registry!(
                "num_success_cmds",
                "Total number of commands success",
                &["workload"],
                registry,
            )
            .unwrap(),
            num_error: register_int_counter_vec_with_registry!(
                "num_error",
                "Total number of transaction errors",
                &["workload", "type"],
                registry,
            )
            .unwrap(),
            num_submitted: register_int_counter_vec_with_registry!(
                "num_submitted",
                "Total number of transaction submitted to sui",
                &["workload"],
                registry,
            )
            .unwrap(),
            num_in_flight: register_gauge_vec_with_registry!(
                "num_in_flight",
                "Total number of transaction in flight",
                &["workload"],
                registry,
            )
            .unwrap(),
            latency_s: register_histogram_vec_with_registry!(
                "latency_s",
                "Total time in seconds to return a response",
                &["workload"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latency_squared_s: register_counter_vec_with_registry!(
                "latency_squared_s",
                "Square of total time in seconds to return a response",
                &["workload"],
                registry,
            )
            .unwrap(),
            validators_in_tx_cert: register_int_counter_vec_with_registry!(
                "validators_in_tx_cert",
                "Number of times a validator was included in tx cert",
                &["validator"],
                registry,
            )
            .unwrap(),
            validators_in_effects_cert: register_int_counter_vec_with_registry!(
                "validators_in_effects_cert",
                "Number of times a validator was included in effects cert",
                &["validator"],
                registry,
            )
            .unwrap(),
            cpu_usage: register_gauge_vec_with_registry!(
                "cpu_usage",
                "CPU usage per core",
                &["cpu"],
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Default)]
struct Stats {
    pub id: usize,
    pub num_no_gas: u64,
    pub num_submitted: u64,
    pub num_in_flight: u64,
    pub bench_stats: BenchmarkStats,
}

type RetryType = Box<(Transaction, Box<dyn Payload>)>;

enum NextOp {
    Response {
        /// Time taken to execute the tx and produce effects
        latency: Duration,
        /// Number of commands in the executed transction
        num_commands: u16,
        /// Gas used in the executed transction
        gas_used: u64,
        /// The payload updated with the effects of the transaction
        payload: Box<dyn Payload>,
    },
    // The transaction failed and could not be retried
    Failure,
    Retry(RetryType),
}

async fn print_and_start_benchmark() -> &'static Instant {
    static ONCE: OnceCell<Instant> = OnceCell::const_new();
    ONCE.get_or_init(|| async move {
        info!("Starting benchmark!");
        Instant::now()
    })
    .await
}

pub struct BenchWorker {
    pub id: u64,
    pub target_qps: u64,
    pub payload: Vec<Box<dyn Payload>>,
    pub proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    pub group: u32,
    pub duration: Interval,
}

impl Debug for BenchWorker {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            format!(
                "BenchWorker id:{}, group:{}, duration:{}, target_qps:{}",
                self.id, self.group, self.duration, self.target_qps
            )
            .as_str(),
        )
    }
}

pub struct BenchDriver {
    pub stat_collection_interval: u64,
    pub stress_stat_collection: bool,
    pub start_time: Instant,
    pub token: CancellationToken,
}

impl BenchDriver {
    pub fn new(stat_collection_interval: u64, stress_stat_collection: bool) -> BenchDriver {
        BenchDriver {
            stat_collection_interval,
            stress_stat_collection,
            start_time: Instant::now(),
            token: CancellationToken::new(),
        }
    }
    pub fn terminate(&self) {
        self.token.cancel()
    }
    pub fn update_progress(
        start_time: Instant,
        interval: Interval,
        gas_used: u64,
        increment_by_value: u64,
        progress_bar: Arc<ProgressBar>,
    ) {
        match interval {
            Interval::Count(count) => {
                progress_bar.inc(increment_by_value);
                if progress_bar.position() >= count {
                    progress_bar.finish_and_clear();
                }
            }
            Interval::Time(Duration::MAX) => progress_bar.inc(increment_by_value),
            Interval::Time(duration) => {
                let elapsed_secs = (Instant::now() - start_time).as_secs();
                progress_bar.set_position(std::cmp::min(duration.as_secs(), elapsed_secs));
                if progress_bar.position() >= duration.as_secs() {
                    progress_bar.finish_and_clear();
                }
            }
        }
        progress_bar.set_message(format!("Gas Used: {gas_used}"));
    }
    pub async fn make_workers(
        &self,
        id: &mut u64,
        workload_info: &WorkloadInfo,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<BenchWorker> {
        let mut workers = vec![];
        let mut qps = workload_info.workload_params.target_qps;
        if qps == 0 {
            return vec![];
        }
        let mut payloads = workload_info
            .workload
            .make_test_payloads(proxy.clone(), system_state_observer.clone())
            .await;
        let mut total_workers = workload_info.workload_params.num_workers;
        while total_workers > 0 {
            let target_qps = qps / total_workers;
            if target_qps > 0 {
                let chunk_size = payloads.len() / total_workers as usize;
                let remaining = payloads.split_off(chunk_size);
                workers.push(BenchWorker {
                    id: *id,
                    target_qps,
                    payload: payloads,
                    proxy: proxy.clone(),
                    group: workload_info.workload_params.group,
                    duration: workload_info.workload_params.duration,
                });
                payloads = remaining;
                qps -= target_qps;
                *id += 1;
            }
            total_workers -= 1;
        }
        workers
    }
}

#[cfg(not(msim))]
async fn ctrl_c() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}

// TODO: if more use is made of tokio::signal we should just add support for it to the sim.
#[cfg(msim)]
async fn ctrl_c() -> std::io::Result<()> {
    futures::future::pending().await
}

#[async_trait]
impl Driver<(BenchmarkStats, StressStats)> for BenchDriver {
    async fn run(
        &self,
        proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
        workloads_by_group_id: BTreeMap<GroupID, Vec<WorkloadInfo>>,
        system_state_observer: Arc<SystemStateObserver>,
        registry: &Registry,
        show_progress: bool,
        total_benchmark_run_interval: Interval,
    ) -> Result<(BenchmarkStats, StressStats), anyhow::Error> {
        info!("Running BenchDriver");

        let mut tasks = Vec::new();
        let (tx, mut rx) = channel(100);
        let (stress_stat_tx, mut stress_stat_rx) = channel(100);

        // All the benchmark workers that are grouped by GroupID. The group is order in group id order
        // ascending. This is important as benchmark groups should be executed in order to follow the input settings.
        let mut bench_workers = VecDeque::new();

        let mut worker_id = 0;
        let mut num_workers = 0;

        for (_, workloads) in workloads_by_group_id.iter() {
            let mut workers = vec![];

            for workload in workloads {
                let proxy = proxies
                    .choose(&mut rand::thread_rng())
                    .context("Failed to get proxy for bench driver")?;
                workers.extend(
                    self.make_workers(
                        &mut worker_id,
                        workload,
                        proxy.clone(),
                        system_state_observer.clone(),
                    )
                    .await,
                );
            }

            num_workers += workers.len();
            bench_workers.push_back(workers);
        }

        if bench_workers.is_empty() {
            return Err(anyhow!("No workers to run benchmark!"));
        }
        let stat_delay_micros = 1_000_000 * self.stat_collection_interval;
        let metrics = Arc::new(BenchMetrics::new(registry));
        let total_benchmark_progress = Arc::new(create_progress_bar(total_benchmark_run_interval));
        let total_benchmark_gas_used = Arc::new(AtomicU64::new(0));

        // Spin up the scheduler task to orchestrate running the workers for each benchmark group.
        let scheduler = spawn_workers_scheduler(
            bench_workers,
            self.token.clone(),
            total_benchmark_progress.clone(),
            total_benchmark_gas_used,
            tx.clone(),
            metrics.clone(),
            total_benchmark_run_interval,
            stat_delay_micros,
        )
        .await;

        tasks.push(scheduler);

        let benchmark_stat_task = tokio::spawn(async move {
            let mut benchmark_stat = BenchmarkStats {
                duration: Duration::ZERO,
                num_error_txes: 0,
                num_success_txes: 0,
                num_expected_error_txes: 0,
                num_success_cmds: 0,
                total_gas_used: 0,
                latency_ms: HistogramWrapper {
                    histogram: hdrhistogram::Histogram::<u64>::new_with_max(120_000, 3).unwrap(),
                },
            };
            let mut stat_collection: BTreeMap<usize, Stats> = BTreeMap::new();
            let mut counter = 0;
            let mut stat;
            let start = Instant::now();
            while let Some(
                sample_stat @ Stats {
                    id,
                    num_no_gas: _,
                    num_in_flight: _,
                    num_submitted: _,
                    bench_stats: _,
                },
            ) = rx.recv().await
            {
                // We use the special id as signal to clear up the stat collection map since that means
                // that new benchmark group workers have spun up.
                if id == usize::MAX {
                    stat_collection.clear();
                    continue;
                }

                benchmark_stat.update(start.elapsed(), &sample_stat.bench_stats);
                stat_collection.insert(id, sample_stat);

                let mut total_qps: f32 = 0.0;
                let mut total_cps: f32 = 0.0;
                let mut num_success_txes: u64 = 0;
                let mut num_error_txes: u64 = 0;
                let mut num_expected_error_txes: u64 = 0;
                let mut num_success_cmds = 0;
                let mut latency_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(120_000, 3).unwrap();

                let mut num_in_flight: u64 = 0;
                let mut num_submitted: u64 = 0;
                let mut num_no_gas = 0;
                for (_, v) in stat_collection.iter() {
                    let duration = v.bench_stats.duration.as_secs() as f32;

                    // no reason to do any measurements when duration is zero as this will output NaN
                    if duration == 0.0 {
                        continue;
                    }

                    total_qps += v.bench_stats.num_success_txes as f32 / duration;
                    total_cps += v.bench_stats.num_success_cmds as f32 / duration;
                    num_success_txes += v.bench_stats.num_success_txes;
                    num_error_txes += v.bench_stats.num_error_txes;
                    num_expected_error_txes += v.bench_stats.num_expected_error_txes;
                    num_success_cmds += v.bench_stats.num_success_cmds;
                    num_no_gas += v.num_no_gas;
                    num_submitted += v.num_submitted;
                    num_in_flight += v.num_in_flight;
                    latency_histogram
                        .add(&v.bench_stats.latency_ms.histogram)
                        .unwrap();
                }
                let denom = num_success_txes + num_error_txes;
                let _error_rate = if denom > 0 {
                    num_error_txes as f32 / denom as f32
                } else {
                    0.0
                };
                counter += 1;
                if counter % num_workers == 0 {
                    stat = format!(
                        "TPS = {}, CPS = {}, latency_ms(min/p50/p99/max) = {}/{}/{}/{}, \
                        num_success_tx = {}, num_error_tx = {}, num_expected_error_tx = {}, \
                        num_success_cmds = {}, no_gas = {}, submitted = {}, in_flight = {}",
                        total_qps,
                        total_cps,
                        latency_histogram.min(),
                        latency_histogram.value_at_quantile(0.5),
                        latency_histogram.value_at_quantile(0.99),
                        latency_histogram.max(),
                        num_success_txes,
                        num_error_txes,
                        num_expected_error_txes,
                        num_success_cmds,
                        num_no_gas,
                        num_submitted,
                        num_in_flight
                    );
                    if show_progress {
                        eprintln!("{}", stat);
                    }
                }
            }
            benchmark_stat
        });
        drop(tx);

        if self.stress_stat_collection {
            tasks.push(stress_stats_collector(
                total_benchmark_progress.clone(),
                metrics.clone(),
                stress_stat_tx.clone(),
            ));
        }
        drop(stress_stat_tx);

        let stress_stat_task = tokio::spawn(async move {
            let mut stress_stat = StressStats {
                cpu_usage: HistogramWrapper {
                    histogram: hdrhistogram::Histogram::<u64>::new_with_max(100, 3).unwrap(),
                },
            };
            let mut stat_collection: Vec<StressStats> = Vec::new();
            let mut counter = 0;
            while let Some(sample_stat @ StressStats { cpu_usage: _ }) = stress_stat_rx.recv().await
            {
                stress_stat.update(&sample_stat);
                stat_collection.push(sample_stat);

                let mut cpu_usage_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(100, 3).unwrap();
                for stat in stat_collection.iter() {
                    cpu_usage_histogram.add(&stat.cpu_usage.histogram).unwrap();
                }
                counter += 1;
                if counter % num_workers == 0 {
                    let stat = format!(
                        "cpu_usage p50 = {}, p99 = {}",
                        cpu_usage_histogram.value_at_quantile(0.5),
                        cpu_usage_histogram.value_at_quantile(0.99)
                    );
                    if show_progress {
                        eprintln!("{}", stat);
                    }
                }
            }
            stress_stat
        });

        let all_tasks = try_join_all(tasks);
        let _res = tokio::select! {
            _ = ctrl_c() => {
                self.terminate();
                vec![]
            }
            res = all_tasks => res.unwrap().into_iter().collect()
        };
        let benchmark_stat = benchmark_stat_task.await.unwrap();
        let stress_stat = stress_stat_task.await.unwrap();
        Ok((benchmark_stat, stress_stat))
    }
}

/// The workers scheduler is orchestrating the bench workers to run according to their group. Each
/// group is running for a specific period/interval. Once finished then the next group of bench workers
/// is picked up to run. The worker groups are cycled , so once the last group is run then we start
/// again from the beginning. That allows running benchmarks with repeatable patterns across the whole
/// benchmark duration.
async fn spawn_workers_scheduler(
    mut bench_workers: VecDeque<Vec<BenchWorker>>,
    cancellation_token: CancellationToken,
    total_benchmark_progress_cloned: Arc<ProgressBar>,
    total_benchmark_gas_used: Arc<AtomicU64>,
    tx_cloned: Sender<Stats>,
    metrics_cloned: Arc<BenchMetrics>,
    total_benchmark_run_interval: Interval,
    stat_delay_micros: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("Spawn up scheduler task...");

        let mut running_workers: JoinSet<Option<BenchWorker>> = JoinSet::new();
        let mut finished_workers = Vec::new();
        let (tx_workers_to_run, mut rx_workers_to_run) = channel(1);

        let mut check_interval = time::interval(Duration::from_millis(500));
        check_interval.set_missed_tick_behavior(time::MissedTickBehavior::Burst);

        // Set the total benchmark start time
        let total_benchmark_start_time = print_and_start_benchmark().await;

        // Initially boostrap the first tasks
        tx_workers_to_run
            .send(bench_workers.pop_front().unwrap())
            .await
            .expect("Should be able to send next workers to run");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                },
                // Consume the next running worker that has been finished. When all finished
                // be ready to spin up the next workers - if not total benchmark already finished.
                Some(result) = running_workers.join_next() => {
                    if total_benchmark_progress_cloned.is_finished() {
                        info!("Benchmark finished, now exiting the scheduler loop");
                        break;
                    }

                    let worker = if let Some(worker) = result.expect("Worker tasks should shutdown gracefully") {
                        worker
                    } else {
                        warn!("No worker returned by task - that means it has been cancelled and we are exiting.");
                        return;
                    };
                    finished_workers.push(worker);

                    // If workers have all finished, then we can progress to the next group run, if
                    // any exists
                    if running_workers.is_empty() {
                        bench_workers.push_back(finished_workers);

                        finished_workers = Vec::new();

                        // cycle through the workers set
                        tx_workers_to_run.send(bench_workers.pop_front().unwrap()).await.expect("Should be able to send next workers to run");
                    }
                },
                // Next workers to run
                Some(workers) = rx_workers_to_run.recv() => {
                    // clear up previous stats map by sending a special stat with MAX id
                    let _ = tx_cloned.send(Stats {
                            id: usize::MAX,
                            ..Stats::default()
                        }).await;

                    let futures = spawn_bench_workers(
                        workers,
                        metrics_cloned.clone(),
                        tx_cloned.clone(),
                        cancellation_token.clone(),
                        stat_delay_micros,
                        total_benchmark_progress_cloned.clone(),
                        total_benchmark_run_interval,
                        *total_benchmark_start_time,
                        total_benchmark_gas_used.clone()
                    )
                    .await;

                    for f in futures {
                        running_workers.spawn(f);
                    }
                },
                // Check every now and then if the overall benchmark has been finished
                _ = check_interval.tick() => {
                    if total_benchmark_progress_cloned.is_finished() {
                        info!("Benchmark finished, now exiting the scheduler loop");
                        break;
                    }
                }
            }
        }
    })
}

async fn spawn_bench_workers(
    workers: Vec<BenchWorker>,
    metrics: Arc<BenchMetrics>,
    tx: Sender<Stats>,
    token: CancellationToken,
    stat_delay_micros: u64,
    total_benchmark_progress: Arc<ProgressBar>,
    total_benchmark_run_interval: Interval,
    total_benchmark_start_time: Instant,
    total_benchmark_gas_used: Arc<AtomicU64>,
) -> Vec<impl Future<Output = Option<BenchWorker>>> {
    // create a barrier to be used for all the spawned workers.
    let barrier = Arc::new(Barrier::new(workers.len()));
    let mut progress_bar = None;
    let mut futures = vec![];
    let num_of_workers = workers.len();
    let group_gas_used = Arc::new(AtomicU64::new(0));

    for worker in workers {
        if progress_bar.as_ref().is_none() {
            progress_bar = Some(Arc::new(create_progress_bar(worker.duration)));
            info!(
                "Spawning workers {} for benchmark group {} with duration {}...",
                num_of_workers, worker.group, worker.duration
            );
        }

        let f = run_bench_worker(
            barrier.clone(),
            metrics.clone(),
            tx.clone(),
            token.clone(),
            stat_delay_micros,
            worker,
            progress_bar.as_ref().unwrap().clone(),
            group_gas_used.clone(),
            total_benchmark_progress.clone(),
            total_benchmark_run_interval,
            total_benchmark_start_time,
            total_benchmark_gas_used.clone(),
        );

        futures.push(f);
    }

    futures
}

async fn run_bench_worker(
    barrier: Arc<Barrier>,
    metrics_cloned: Arc<BenchMetrics>,
    tx_cloned: Sender<Stats>,
    cloned_token: CancellationToken,
    stat_delay_micros: u64,
    mut worker: BenchWorker,
    group_benchmark_progress: Arc<ProgressBar>,
    group_gas_used: Arc<AtomicU64>,
    total_benchmark_progress: Arc<ProgressBar>,
    total_benchmark_run_interval: Interval,
    total_benchmark_start_time: Instant,
    total_benchmark_gas_used: Arc<AtomicU64>,
) -> Option<BenchWorker> {
    // Waiting until all the tasks have been spawn , so we can coordinate the traffic and timing.
    barrier.wait().await;
    debug!("Run {:?}", worker);
    let group_benchmark_start_time = Instant::now();

    let request_delay_micros = 1_000_000 / worker.target_qps;
    let mut num_success_txes = 0;
    let mut num_error_txes = 0;
    let mut num_expected_error_txes = 0;
    let mut num_success_cmds = 0;
    let mut num_no_gas = 0;
    let mut num_in_flight: u64 = 0;
    let mut num_submitted = 0;
    let mut worker_gas_used = 0;

    let mut latency_histogram = hdrhistogram::Histogram::<u64>::new_with_max(120_000, 3).unwrap();
    let mut request_interval = time::interval(Duration::from_micros(request_delay_micros));
    request_interval.set_missed_tick_behavior(time::MissedTickBehavior::Burst);
    let mut stat_interval = time::interval(Duration::from_micros(stat_delay_micros));

    let mut retry_queue: VecDeque<RetryType> = VecDeque::new();

    let group_benchmark_run_interval = worker.duration;
    let mut free_pool: VecDeque<_> = worker.payload.into_iter().collect();

    let mut stat_start_time: Instant = Instant::now();

    // Handles the transaction response when sent to proxy.
    let handle_execute_transaction_response = |result: Result<ExecutionEffects>,
                                               start: Arc<Instant>,
                                               transaction: Transaction,
                                               mut payload: Box<dyn Payload>,
                                               committee: Arc<Committee>|
     -> NextOp {
        match result {
            Ok(effects) => {
                assert!(
                    payload.get_failure_type().is_none()
                        || payload.get_failure_type() == Some(ExpectedFailureType::NoFailure)
                );
                let latency = start.elapsed();
                let time_from_start = total_benchmark_start_time.elapsed();

                metrics_cloned
                    .benchmark_duration
                    .set(time_from_start.as_secs() as i64);

                let square_latency_ms = latency.as_secs_f64().powf(2.0);
                metrics_cloned
                    .latency_s
                    .with_label_values(&[&payload.to_string()])
                    .observe(latency.as_secs_f64());
                metrics_cloned
                    .latency_squared_s
                    .with_label_values(&[&payload.to_string()])
                    .inc_by(square_latency_ms);
                metrics_cloned
                    .num_in_flight
                    .with_label_values(&[&payload.to_string()])
                    .dec();

                let num_commands =
                    transaction.data().transaction_data().kind().num_commands() as u16;

                if effects.is_ok() {
                    metrics_cloned
                        .num_success
                        .with_label_values(&[&payload.to_string()])
                        .inc();
                    metrics_cloned
                        .num_success_cmds
                        .with_label_values(&[&payload.to_string()])
                        .inc_by(num_commands as u64);
                } else {
                    metrics_cloned
                        .num_error
                        .with_label_values(&[&payload.to_string(), "execution"])
                        .inc();
                }

                if let Some(sig_info) = effects.quorum_sig() {
                    sig_info.authorities(&committee).for_each(|name| {
                        metrics_cloned
                            .validators_in_effects_cert
                            .with_label_values(&[&name.unwrap().to_string()])
                            .inc()
                    })
                }

                payload.make_new_payload(&effects);
                NextOp::Response {
                    latency,
                    num_commands,
                    payload,
                    gas_used: effects.gas_used(),
                }
            }
            Err(err) => {
                tracing::error!(
                    "Transaction execution got error: {}. Transaction digest: {:?}",
                    err,
                    transaction.digest()
                );
                match payload.get_failure_type() {
                    Some(ExpectedFailureType::NoFailure) => {
                        panic!("Transaction failed unexpectedly");
                    }
                    Some(_) => {
                        metrics_cloned
                            .num_expected_error
                            .with_label_values(&[&payload.to_string()])
                            .inc();
                        NextOp::Retry(Box::new((transaction, payload)))
                    }
                    None => {
                        if err
                            .downcast::<QuorumDriverError>()
                            .and_then(|err| {
                                if matches!(
                                    err,
                                    QuorumDriverError::NonRecoverableTransactionError { .. }
                                ) {
                                    Err(err.into())
                                } else {
                                    Ok(())
                                }
                            })
                            .is_err()
                        {
                            NextOp::Failure
                        } else {
                            metrics_cloned
                                .num_error
                                .with_label_values(&[&payload.to_string(), "rpc"])
                                .inc();
                            NextOp::Retry(Box::new((transaction, payload)))
                        }
                    }
                }
            }
        }
    };

    // Updates the progress bars. if any of the progress bars are finished then true is returned. False otherwise.
    let update_progress = |increment_by_value: u64| {
        let group_gas_used = group_gas_used.load(Ordering::SeqCst);
        let total_benchmark_gas_used = total_benchmark_gas_used.load(Ordering::SeqCst);

        // Update progress for total benchmark
        BenchDriver::update_progress(
            total_benchmark_start_time,
            total_benchmark_run_interval,
            total_benchmark_gas_used,
            increment_by_value,
            total_benchmark_progress.clone(),
        );
        if total_benchmark_progress.is_finished() {
            return true;
        }

        // Update progress for group benchmark
        BenchDriver::update_progress(
            group_benchmark_start_time,
            group_benchmark_run_interval,
            group_gas_used,
            increment_by_value,
            group_benchmark_progress.clone(),
        );
        if group_benchmark_progress.is_finished() {
            return true;
        }
        false
    };

    let mut futures: FuturesUnordered<BoxFuture<NextOp>> = FuturesUnordered::new();

    loop {
        tokio::select! {
            _ = cloned_token.cancelled() => {
                return None;
            }
            _ = stat_interval.tick() => {
                if tx_cloned
                    .try_send(Stats {
                        id: worker.id as usize,
                        num_no_gas,
                        num_in_flight,
                        num_submitted,
                        bench_stats: BenchmarkStats {
                            duration:stat_start_time.elapsed(),
                            num_error_txes,
                            num_expected_error_txes,
                            num_success_txes,
                            num_success_cmds,
                            latency_ms:HistogramWrapper{
                                histogram:latency_histogram.clone()
                            },
                            total_gas_used: worker_gas_used
                        },
                    })
                    .is_err()
                {
                    debug!("Failed to update stat!");
                }
                num_success_txes = 0;
                num_error_txes = 0;
                num_expected_error_txes = 0;
                num_success_cmds = 0;
                num_no_gas = 0;
                num_submitted = 0;
                worker_gas_used = 0;
                stat_start_time = Instant::now();
                latency_histogram.reset();
            }
            _ = request_interval.tick() => {

                // Update progress for total benchmark
                if update_progress(0) {
                    break;
                }

                // If a retry is available send that
                // (sending retries here subjects them to our rate limit)
                if let Some(b) = retry_queue.pop_front() {
                    let tx = b.0;
                    let payload = b.1;
                    match payload.get_failure_type() {
                        Some(ExpectedFailureType::NoFailure) => num_error_txes += 1,
                        Some(_) => num_expected_error_txes += 1,
                        None => num_error_txes += 1,
                    }
                    num_submitted += 1;
                    metrics_cloned.num_submitted.with_label_values(&[&payload.to_string()]).inc();
                    // TODO: clone committee for each request is not ideal.
                    let committee = worker.proxy.clone_committee();
                    let start = Arc::new(Instant::now());
                    let res = worker.proxy
                        .execute_transaction_block(tx.clone())
                        .then(|res| async move  {
                             handle_execute_transaction_response(res, start, tx, payload, committee)
                        });
                    futures.push(Box::pin(res));
                    continue
                }

                // Otherwise send a fresh request
                if free_pool.is_empty() {
                    num_no_gas += 1;
                } else {
                    let mut payload = free_pool.pop_front().unwrap();
                    num_in_flight += 1;
                    num_submitted += 1;
                    metrics_cloned.num_in_flight.with_label_values(&[&payload.to_string()]).inc();
                    metrics_cloned.num_submitted.with_label_values(&[&payload.to_string()]).inc();
                    let tx = payload.make_transaction();
                    let start = Arc::new(Instant::now());
                    // TODO: clone committee for each request is not ideal.
                    let committee = worker.proxy.clone_committee();
                    let res = worker.proxy
                        .execute_transaction_block(tx.clone())
                    .then(|res| async move {
                        handle_execute_transaction_response(res, start, tx, payload, committee)
                    });
                    futures.push(Box::pin(res));
                }
            }
            Some(op) = futures.next() => {
                match op {
                    NextOp::Retry(b) => {
                        retry_queue.push_back(b);

                        // Update total benchmark progress
                        if update_progress(1) {
                            break;
                        }
                    }
                    NextOp::Failure => {
                        error!("Permanent failure to execute payload. May result in gas objects being leaked");
                        num_error_txes += 1;
                        // Update total benchmark progress
                        if update_progress(1) {
                            break;
                        }
                    }
                    NextOp::Response { latency, num_commands, payload, gas_used } => {
                        num_success_txes += 1;
                        num_success_cmds += num_commands as u64;
                        num_in_flight -= 1;
                        worker_gas_used += gas_used;
                        free_pool.push_back(payload);
                        latency_histogram.saturating_record(latency.as_millis().try_into().unwrap());

                        let _ = group_gas_used.fetch_add(worker_gas_used, Ordering::SeqCst);
                        let _ = total_benchmark_gas_used.fetch_add(worker_gas_used, Ordering::SeqCst);

                        // Update total benchmark progress
                        if update_progress(1) {
                            break;
                        }
                    }
                }
            }
        }
    }
    // send stats one last time
    if tx_cloned
        .try_send(Stats {
            id: worker.id as usize,
            num_no_gas,
            num_in_flight,
            num_submitted,
            bench_stats: BenchmarkStats {
                duration: stat_start_time.elapsed(),
                num_error_txes,
                num_expected_error_txes,
                num_success_txes,
                num_success_cmds,
                total_gas_used: worker_gas_used,
                latency_ms: HistogramWrapper {
                    histogram: latency_histogram,
                },
            },
        })
        .is_err()
    {
        debug!("Failed to update stat!");
    }

    // Wait for futures to complete so we can get the remaining payloads
    info!(
        "Waiting for {} txes to complete for worker id {}...",
        futures.len(),
        worker.id
    );
    while let Some(result) = futures.next().await {
        let p = match result {
            NextOp::Failure => {
                error!(
                    "Permanent failure to execute payload. May result in gas objects being leaked"
                );
                continue;
            }
            NextOp::Response {
                latency: _,
                num_commands: _,
                gas_used: _,
                payload,
            } => payload,
            NextOp::Retry(b) => b.1,
        };
        free_pool.push_back(p);
    }

    // Explicitly drop futures so we can move the worker
    drop(futures);

    worker.payload = free_pool.into_iter().collect();
    Some(worker)
}

/// Creates a new progress bar based on the provided duration. The method is agnostic to the actual
/// usage - weather we want to track the overall benchmark duration or an individual benchmark run.
fn create_progress_bar(duration: Interval) -> ProgressBar {
    fn new_progress_bar(len: u64) -> ProgressBar {
        if cfg!(msim) {
            // don't print any progress when running in the simulator
            ProgressBar::hidden()
        } else {
            ProgressBar::new(len)
        }
    }

    match duration {
        Interval::Count(count) => new_progress_bar(count)
            .with_prefix("Running benchmark(count):")
            .with_style(
                ProgressStyle::with_template("{prefix}: {wide_bar} {pos}/{len}: {msg}").unwrap(),
            ),
        Interval::Time(Duration::MAX) => ProgressBar::hidden(),
        Interval::Time(duration) => new_progress_bar(duration.as_secs())
            .with_prefix("Running benchmark(duration):")
            .with_style(ProgressStyle::with_template("{prefix}: {wide_bar} {pos}/{len}").unwrap()),
    }
}

fn stress_stats_collector(
    progress: Arc<ProgressBar>,
    metrics: Arc<BenchMetrics>,
    stress_stat_tx: Sender<StressStats>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut system = System::new_all();

        system.refresh_cpu();
        tokio::time::sleep(Duration::from_secs(1)).await;

        while !progress.is_finished() {
            let mut cpu_usage_histogram =
                hdrhistogram::Histogram::<u64>::new_with_max(100, 3).unwrap();
            system.refresh_cpu();
            for (i, cpu) in system.cpus().iter().enumerate() {
                cpu_usage_histogram.saturating_record(cpu.cpu_usage() as u64);
                metrics
                    .cpu_usage
                    .with_label_values(&[&format!("cpu_{i}").to_string()])
                    .set(cpu.cpu_usage().into());
            }

            if stress_stat_tx
                .try_send(StressStats {
                    cpu_usage: HistogramWrapper {
                        histogram: cpu_usage_histogram,
                    },
                })
                .is_err()
            {
                debug!("Failed to update stress stats!");
            }

            tokio::select! {
                _ = ctrl_c() => {
                    break;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)) => (),
            }
        }
    })
}
