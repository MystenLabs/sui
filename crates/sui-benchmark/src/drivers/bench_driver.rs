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
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::Registry;
use prometheus::{register_counter_vec_with_registry, register_gauge_vec_with_registry};
use prometheus::{register_histogram_vec_with_registry, register_int_counter_with_registry};
use prometheus::{register_int_counter_vec_with_registry, CounterVec};
use prometheus::{GaugeVec, IntCounter};
use rand::seq::SliceRandom;
use tokio::sync::mpsc::Sender;
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use crate::drivers::driver::Driver;
use crate::drivers::HistogramWrapper;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::WorkloadInfo;
use crate::ValidatorProxy;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use sui_types::messages::{TransactionDataAPI, VerifiedTransaction};
use sysinfo::{CpuExt, System, SystemExt};
use tokio::sync::Barrier;
use tokio::{time, time::Instant};
use tracing::{debug, error, info};

use super::Interval;
use super::{BenchmarkStats, StressStats};
pub struct BenchMetrics {
    pub benchmark_duration: IntCounter,
    pub num_success: IntCounterVec,
    pub num_error: IntCounterVec,
    pub num_submitted: IntCounterVec,
    pub num_in_flight: GaugeVec,
    pub latency_s: HistogramVec,
    pub latency_squared_s: CounterVec,
    pub validators_in_tx_cert: IntCounterVec,
    pub validators_in_effects_cert: IntCounterVec,
    pub cpu_usage: GaugeVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl BenchMetrics {
    fn new(registry: &Registry) -> Self {
        BenchMetrics {
            benchmark_duration: register_int_counter_with_registry!(
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
            num_error: register_int_counter_vec_with_registry!(
                "num_error",
                "Total number of transaction errors",
                &["workload"],
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

struct Stats {
    pub id: usize,
    pub num_no_gas: u64,
    pub num_submitted: u64,
    pub num_in_flight: u64,
    pub bench_stats: BenchmarkStats,
}

type RetryType = Box<(VerifiedTransaction, Box<dyn Payload>)>;

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
    pub target_qps: u64,
    pub payload: Vec<Box<dyn Payload>>,
    pub proxy: Arc<dyn ValidatorProxy + Send + Sync>,
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
        progress_bar: Arc<ProgressBar>,
    ) {
        match interval {
            Interval::Count(count) => {
                progress_bar.inc(1);
                if progress_bar.position() >= count {
                    progress_bar.finish_and_clear();
                }
            }
            Interval::Time(Duration::MAX) => progress_bar.inc(1),
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
                    target_qps,
                    payload: payloads,
                    proxy: proxy.clone(),
                });
                payloads = remaining;
                qps -= target_qps;
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
        workloads: Vec<WorkloadInfo>,
        system_state_observer: Arc<SystemStateObserver>,
        registry: &Registry,
        show_progress: bool,
        run_duration: Interval,
    ) -> Result<(BenchmarkStats, StressStats), anyhow::Error> {
        info!("Running BenchDriver");

        let mut tasks = Vec::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let (stress_stat_tx, mut stress_stat_rx) = tokio::sync::mpsc::channel(100);
        let mut bench_workers = vec![];
        for workload in workloads.iter() {
            let proxy = proxies
                .choose(&mut rand::thread_rng())
                .context("Failed to get proxy for bench driver")?;
            bench_workers.extend(
                self.make_workers(workload, proxy.clone(), system_state_observer.clone())
                    .await,
            );
        }
        let num_workers = bench_workers.len() as u64;
        if num_workers == 0 {
            return Err(anyhow!("No workers to run benchmark!"));
        }
        let stat_delay_micros = 1_000_000 * self.stat_collection_interval;
        let metrics = Arc::new(BenchMetrics::new(registry));
        let barrier = Arc::new(Barrier::new(num_workers as usize));
        info!("Setting up {:?} workers...", num_workers);
        let progress = Arc::new(match run_duration {
            Interval::Count(count) => ProgressBar::new(count)
                .with_prefix("Running benchmark(count):")
                .with_style(
                    ProgressStyle::with_template("{prefix}: {wide_bar} {pos}/{len}: {msg}")
                        .unwrap(),
                ),
            Interval::Time(Duration::MAX) => ProgressBar::hidden(),
            Interval::Time(duration) => ProgressBar::new(duration.as_secs())
                .with_prefix("Running benchmark(duration):")
                .with_style(
                    ProgressStyle::with_template("{prefix}: {wide_bar} {pos}/{len}").unwrap(),
                ),
        });
        for (i, worker) in bench_workers.into_iter().enumerate() {
            let cloned_token = self.token.clone();
            let request_delay_micros = 1_000_000 / worker.target_qps;
            let mut free_pool: VecDeque<_> = worker.payload.into_iter().collect();
            let progress_cloned = progress.clone();
            let tx_cloned = tx.clone();
            let cloned_barrier = barrier.clone();
            let metrics_cloned = metrics.clone();

            let runner = tokio::spawn(async move {
                cloned_barrier.wait().await;
                let start_time = print_and_start_benchmark().await;
                let mut num_success_txes = 0;
                let mut num_error_txes = 0;
                let mut num_success_cmds = 0;
                let mut num_no_gas = 0;
                let mut num_in_flight: u64 = 0;
                let mut num_submitted = 0;
                let mut total_gas_used = 0;
                let mut latency_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(120_000, 3).unwrap();
                let mut request_interval =
                    time::interval(Duration::from_micros(request_delay_micros));
                request_interval.set_missed_tick_behavior(time::MissedTickBehavior::Burst);
                let mut stat_interval = time::interval(Duration::from_micros(stat_delay_micros));
                let mut futures: FuturesUnordered<BoxFuture<NextOp>> = FuturesUnordered::new();

                let mut retry_queue: VecDeque<RetryType> = VecDeque::new();
                let mut stat_start_time: Instant = Instant::now();
                loop {
                    tokio::select! {
                        _ = cloned_token.cancelled() => {
                            break;
                        }
                        _ = stat_interval.tick() => {
                            if tx_cloned
                                .try_send(Stats {
                                    id: i,
                                    num_no_gas,
                                    num_in_flight,
                                    num_submitted,
                                    bench_stats: BenchmarkStats {duration:stat_start_time.elapsed(),num_error_txes,num_success_txes,num_success_cmds,latency_ms:HistogramWrapper{histogram:latency_histogram.clone()}, total_gas_used },
                                })
                                .is_err()
                            {
                                debug!("Failed to update stat!");
                            }
                            num_success_txes = 0;
                            num_error_txes = 0;
                            num_success_cmds = 0;
                            num_no_gas = 0;
                            num_submitted = 0;
                            stat_start_time = Instant::now();
                            latency_histogram.reset();
                        }
                        _ = request_interval.tick() => {

                            // If a retry is available send that
                            // (sending retries here subjects them to our rate limit)
                            if let Some(mut b) = retry_queue.pop_front() {
                                num_error_txes += 1;
                                num_submitted += 1;
                                metrics_cloned.num_submitted.with_label_values(&[&b.1.to_string()]).inc();
                                let metrics_cloned = metrics_cloned.clone();
                                // TODO: clone committee for each request is not ideal.
                                let committee_cloned = Arc::new(worker.proxy.clone_committee());
                                let start = Arc::new(Instant::now());
                                let res = worker.proxy
                                    .execute_transaction_block(b.0.clone().into())
                                    .then(|res| async move  {
                                        match res {
                                            Ok(effects) => {
                                                let latency = start.elapsed();
                                                let time_from_start = start_time.elapsed();

                                                if let Some(delta) = time_from_start.as_secs().checked_sub(metrics_cloned.benchmark_duration.get()) {
                                                    metrics_cloned.benchmark_duration.inc_by(delta);
                                                }

                                                let square_latency_ms = latency.as_secs_f64().powf(2.0);
                                                metrics_cloned.latency_s.with_label_values(&[&b.1.to_string()]).observe(latency.as_secs_f64());
                                                metrics_cloned.latency_squared_s.with_label_values(&[&b.1.to_string()]).inc_by(square_latency_ms);

                                                metrics_cloned.num_success.with_label_values(&[&b.1.to_string()]).inc();
                                                metrics_cloned.num_in_flight.with_label_values(&[&b.1.to_string()]).dec();
                                                // let auth_sign_info = AuthorityStrongQuorumSignInfo::try_from(&cert.auth_sign_info).unwrap();
                                                // auth_sign_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_tx_cert.with_label_values(&[&name.unwrap().to_string()]).inc());
                                                if let Some(sig_info) = effects.quorum_sig() {
                                                    sig_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_effects_cert.with_label_values(&[&name.unwrap().to_string()]).inc())
                                                }
                                                let num_commands = b.0.data().transaction_data().kind().num_commands() as u16;
                                                b.1.make_new_payload(&effects);
                                                NextOp::Response {latency,num_commands,payload:b.1, gas_used: effects.gas_used() }
                                            }
                                            Err(err) => {
                                                error!("{}", err);
                                                metrics_cloned.num_error.with_label_values(&[&b.1.to_string()]).inc();
                                                NextOp::Retry(b)
                                            }
                                        }
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
                                let metrics_cloned = metrics_cloned.clone();
                                // TODO: clone committee for each request is not ideal.
                                let committee_cloned = Arc::new(worker.proxy.clone_committee());
                                let res = worker.proxy
                                    .execute_transaction_block(tx.clone().into())
                                .then(|res| async move {
                                    match res {
                                        Ok(effects) => {
                                            let latency = start.elapsed();
                                            let time_from_start = start_time.elapsed();

                                            if let Some(delta) = time_from_start.as_secs().checked_sub(metrics_cloned.benchmark_duration.get()) {
                                                metrics_cloned.benchmark_duration.inc_by(delta);
                                            }

                                            let square_latency_ms = latency.as_secs_f64().powf(2.0);
                                            metrics_cloned.latency_s.with_label_values(&[&payload.to_string()]).observe(latency.as_secs_f64());
                                            metrics_cloned.latency_squared_s.with_label_values(&[&payload.to_string()]).inc_by(square_latency_ms);

                                            metrics_cloned.num_success.with_label_values(&[&payload.to_string()]).inc();
                                            metrics_cloned.num_in_flight.with_label_values(&[&payload.to_string()]).dec();
                                            // let auth_sign_info = AuthorityStrongQuorumSignInfo::try_from(&cert.auth_sign_info).unwrap();
                                            // auth_sign_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_tx_cert.with_label_values(&[&name.unwrap().to_string()]).inc());
                                            if let Some(sig_info) = effects.quorum_sig() { sig_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_effects_cert.with_label_values(&[&name.unwrap().to_string()]).inc()) }
                                            payload.make_new_payload(&effects);
                                            let num_commands = tx.data().transaction_data().kind().num_commands() as u16;
                                            NextOp::Response {latency,num_commands,payload, gas_used: effects.gas_used() }
                                        }
                                        Err(err) => {
                                            error!("Retry due to error: {}", err);
                                            metrics_cloned.num_error.with_label_values(&[&payload.to_string()]).inc();
                                            NextOp::Retry(Box::new((tx, payload)))
                                        }
                                    }
                                });
                                futures.push(Box::pin(res));
                            }
                        }
                        Some(op) = futures.next() => {
                            match op {
                                NextOp::Retry(b) => {
                                    retry_queue.push_back(b);
                                    BenchDriver::update_progress(*start_time, run_duration, total_gas_used, progress_cloned.clone());
                                    if progress_cloned.is_finished() {
                                        break;
                                    }
                                }
                                NextOp::Response { latency, num_commands, payload, gas_used } => {
                                    num_success_txes += 1;
                                    num_success_cmds += num_commands as u64;
                                    num_in_flight -= 1;
                                    total_gas_used += gas_used;
                                    free_pool.push_back(payload);
                                    latency_histogram.saturating_record(latency.as_millis().try_into().unwrap());
                                    BenchDriver::update_progress(*start_time, run_duration, total_gas_used, progress_cloned.clone());
                                    if progress_cloned.is_finished() {
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
                        id: i,
                        num_no_gas,
                        num_in_flight,
                        num_submitted,
                        bench_stats: BenchmarkStats {
                            duration: stat_start_time.elapsed(),
                            num_error_txes,
                            num_success_txes,
                            num_success_cmds,
                            total_gas_used,
                            latency_ms: HistogramWrapper {
                                histogram: latency_histogram,
                            },
                        },
                    })
                    .is_err()
                {
                    debug!("Failed to update stat!");
                }
            });
            tasks.push(runner);
        }

        let benchmark_stat_task = tokio::spawn(async move {
            let mut benchmark_stat = BenchmarkStats {
                duration: Duration::ZERO,
                num_error_txes: 0,
                num_success_txes: 0,
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
                benchmark_stat.update(start.elapsed(), &sample_stat.bench_stats);
                stat_collection.insert(id, sample_stat);
                let mut total_qps: f32 = 0.0;
                let mut total_cps: f32 = 0.0;
                let mut num_success_txes: u64 = 0;
                let mut num_error_txes: u64 = 0;
                let mut num_success_cmds = 0;
                let mut latency_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(120_000, 3).unwrap();

                let mut num_in_flight: u64 = 0;
                let mut num_submitted: u64 = 0;
                let mut num_no_gas = 0;
                for (_, v) in stat_collection.iter() {
                    let duration = v.bench_stats.duration.as_secs() as f32;
                    total_qps += v.bench_stats.num_success_txes as f32 / duration;
                    total_cps += v.bench_stats.num_success_cmds as f32 / duration;
                    num_success_txes += v.bench_stats.num_success_txes;
                    num_error_txes += v.bench_stats.num_error_txes;
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
                    stat = format!("TPS = {}, CPS = {}, latency_ms(min/p50/p99/max) = {}/{}/{}/{}, num_success_tx = {}, num_error_tx = {}, num_success_cmds = {}, no_gas = {}, submitted = {}, in_flight = {}", total_qps, total_cps, latency_histogram.min(), latency_histogram.value_at_quantile(0.5), latency_histogram.value_at_quantile(0.99), latency_histogram.max(), num_success_txes, num_error_txes, num_success_cmds, num_no_gas, num_submitted, num_in_flight);
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
                progress.clone(),
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
