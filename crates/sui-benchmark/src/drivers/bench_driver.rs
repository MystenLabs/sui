// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::try_join_all;
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use prometheus::register_gauge_vec_with_registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::GaugeVec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::Registry;
use rand::Rng;
use tokio::sync::OnceCell;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::drivers::driver::Driver;
use crate::drivers::HistogramWrapper;
use crate::workloads::workload::Payload;
use crate::workloads::workload::WorkloadInfo;
use crate::ValidatorProxy;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use sui_types::messages::VerifiedTransaction;
use tokio::sync::Barrier;
use tokio::time;
use tokio::time::Instant;
use tracing::{debug, error};

use super::BenchmarkStats;
use super::Interval;
pub struct BenchMetrics {
    pub num_success: IntCounterVec,
    pub num_error: IntCounterVec,
    pub num_submitted: IntCounterVec,
    pub num_in_flight: GaugeVec,
    pub latency_s: HistogramVec,
    pub validators_in_tx_cert: IntCounterVec,
    pub validators_in_effects_cert: IntCounterVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

const RECONFIG_QUIESCENCE_TIME_SEC: u64 = 10;

impl BenchMetrics {
    fn new(registry: &Registry) -> Self {
        BenchMetrics {
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
                &["workload", "error_type"],
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
    Response(Option<(Duration, Box<dyn Payload>)>),
    Retry(RetryType),
}

async fn print_and_start_benchmark() -> &'static Instant {
    static ONCE: OnceCell<Instant> = OnceCell::const_new();
    ONCE.get_or_init(|| async move {
        eprintln!("Starting benchmark!");
        Instant::now()
    })
    .await
}

pub struct BenchWorker {
    pub num_requests: u64,
    pub target_qps: u64,
    pub payload: Vec<Box<dyn Payload>>,
}

pub struct BenchDriver {
    pub stat_collection_interval: u64,
    pub start_time: Instant,
    pub token: CancellationToken,
}

impl BenchDriver {
    pub fn new(stat_collection_interval: u64) -> BenchDriver {
        BenchDriver {
            stat_collection_interval,
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
    }
    pub async fn make_workers(
        &self,
        workload_info: &WorkloadInfo,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<BenchWorker> {
        let mut num_requests = workload_info.max_in_flight_ops / workload_info.num_workers;
        let mut target_qps = workload_info.target_qps / workload_info.num_workers;
        let mut workers = vec![];
        for i in 0..workload_info.num_workers {
            if i == workload_info.num_workers - 1 {
                num_requests =
                    workload_info.max_in_flight_ops - workers.len() as u64 * num_requests;
                target_qps = workload_info.target_qps - workers.len() as u64 * target_qps;
            }
            if num_requests > 0 && target_qps > 0 {
                workers.push(BenchWorker {
                    num_requests,
                    target_qps,
                    payload: workload_info
                        .workload
                        .make_test_payloads(num_requests, proxy.clone())
                        .await,
                });
            }
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
impl Driver<BenchmarkStats> for BenchDriver {
    async fn run(
        &self,
        workloads: Vec<WorkloadInfo>,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        registry: &Registry,
        show_progress: bool,
        run_duration: Interval,
    ) -> Result<BenchmarkStats, anyhow::Error> {
        let mut tasks = Vec::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let mut bench_workers = vec![];
        for workload in workloads.iter() {
            bench_workers.extend(self.make_workers(workload, proxy.clone()).await);
        }
        let num_workers = bench_workers.len() as u64;
        if num_workers == 0 {
            return Err(anyhow!("No workers to run benchmark!"));
        }
        let stat_delay_micros = 1_000_000 * self.stat_collection_interval;
        let metrics = Arc::new(BenchMetrics::new(registry));
        let barrier = Arc::new(Barrier::new(num_workers as usize));
        eprintln!("Setting up workers...");
        let progress = Arc::new(match run_duration {
            Interval::Count(count) => ProgressBar::new(count)
                .with_prefix("Running benchmark(count):")
                .with_style(
                    ProgressStyle::with_template("{prefix}: {wide_bar} {pos}/{len}").unwrap(),
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
            let mut free_pool = worker.payload;
            let progress = progress.clone();
            let tx_cloned = tx.clone();
            let cloned_barrier = barrier.clone();
            let metrics_cloned = metrics.clone();

            // Make a per worker proxy, otherwise they all share the same task.
            // For remote proxy, this call is a no-op
            let proxy = Arc::new(proxy.clone_new());

            let runner = tokio::spawn(async move {
                cloned_barrier.wait().await;
                let start_time = print_and_start_benchmark().await;
                let mut num_success = 0;
                let mut num_error = 0;
                let mut num_no_gas = 0;
                let mut num_in_flight: u64 = 0;
                let mut num_submitted = 0;
                let mut latency_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
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
                                    id: i as usize,
                                    num_no_gas,
                                    num_in_flight,
                                    num_submitted,
                                    bench_stats: BenchmarkStats {
                                        duration: stat_start_time.elapsed(),
                                        num_error,
                                        num_success,
                                        latency_ms: HistogramWrapper {histogram: latency_histogram.clone()},
                                    },
                                })
                                .is_err()
                            {
                                debug!("Failed to update stat!");
                            }
                            num_success = 0;
                            num_error = 0;
                            num_no_gas = 0;
                            num_submitted = 0;
                            stat_start_time = Instant::now();
                            latency_histogram.reset();
                        }
                        _ = request_interval.tick() => {

                            // If a retry is available send that
                            // (sending retries here subjects them to our rate limit)
                            if let Some(b) = retry_queue.pop_front() {
                                num_error += 1;
                                num_submitted += 1;
                                metrics_cloned.num_submitted.with_label_values(&[&b.1.get_workload_type().to_string()]).inc();
                                let metrics_cloned = metrics_cloned.clone();
                                // TODO: clone committee for each request is not ideal.
                                let committee_cloned = Arc::new(proxy.clone_committee());
                                let proxy_clone = proxy.clone();
                                let start = Arc::new(Instant::now());
                                let res = proxy
                                    .execute_transaction(b.0.clone().into())
                                    .then(|res| async move  {
                                        match res {
                                            Ok((cert, effects)) => {
                                                let new_version = effects.mutated().iter().find(|(object_ref, _)| {
                                                    object_ref.0 == b.1.get_object_id()
                                                }).map(|x| x.0).unwrap();
                                                let latency = start.elapsed();
                                                metrics_cloned.latency_s.with_label_values(&[&b.1.get_workload_type().to_string()]).observe(latency.as_secs_f64());
                                                metrics_cloned.num_success.with_label_values(&[&b.1.get_workload_type().to_string()]).inc();
                                                metrics_cloned.num_in_flight.with_label_values(&[&b.1.get_workload_type().to_string()]).dec();
                                                cert.auth_sign_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_tx_cert.with_label_values(&[&name.unwrap().to_string()]).inc());
                                                if let Some(sig_info) = effects.quorum_sig() {
                                                    sig_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_effects_cert.with_label_values(&[&name.unwrap().to_string()]).inc())
                                                }
                                                NextOp::Response(Some((
                                                    latency,
                                                    b.1.make_new_payload(new_version, effects.gas_object().0),
                                                ),
                                                ))
                                            }
                                            Err(err) => {
                                                if err.indicates_epoch_change() {
                                                    let mut rng = rand::rngs::OsRng;
                                                    let jitter = rng.gen_range(0..RECONFIG_QUIESCENCE_TIME_SEC);
                                                    sleep(Duration::from_secs(RECONFIG_QUIESCENCE_TIME_SEC + jitter)).await;

                                                    proxy_clone.reconfig().await;
                                                } else {
                                                    error!("{}", err);
                                                    metrics_cloned.num_error.with_label_values(&[&b.1.get_workload_type().to_string(), err.as_ref()]).inc();
                                                }
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
                                let payload = free_pool.pop().unwrap();
                                num_in_flight += 1;
                                num_submitted += 1;
                                metrics_cloned.num_in_flight.with_label_values(&[&payload.get_workload_type().to_string()]).inc();
                                metrics_cloned.num_submitted.with_label_values(&[&payload.get_workload_type().to_string()]).inc();
                                let tx = payload.make_transaction();
                                let start = Arc::new(Instant::now());
                                let metrics_cloned = metrics_cloned.clone();
                                let proxy_clone = proxy.clone();
                                // TODO: clone committee for each request is not ideal.
                                let committee_cloned = Arc::new(proxy.clone_committee());
                                let res = proxy
                                    .execute_transaction(tx.clone().into())
                                .then(|res| async move {
                                    match res {
                                        Ok((cert, effects)) => {
                                            let new_version = effects.mutated().iter().find(|(object_ref, _)| {
                                                object_ref.0 == payload.get_object_id()
                                            }).map(|x| x.0).unwrap();
                                            let latency = start.elapsed();
                                            metrics_cloned.latency_s.with_label_values(&[&payload.get_workload_type().to_string()]).observe(latency.as_secs_f64());
                                            metrics_cloned.num_success.with_label_values(&[&payload.get_workload_type().to_string()]).inc();
                                            metrics_cloned.num_in_flight.with_label_values(&[&payload.get_workload_type().to_string()]).dec();
                                            cert.auth_sign_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_tx_cert.with_label_values(&[&name.unwrap().to_string()]).inc());
                                            if let Some(sig_info) = effects.quorum_sig() { sig_info.authorities(&committee_cloned).for_each(|name| metrics_cloned.validators_in_effects_cert.with_label_values(&[&name.unwrap().to_string()]).inc()) }
                                            NextOp::Response(Some((
                                                latency,
                                                payload.make_new_payload(new_version, effects.gas_object().0),
                                            )))
                                        }
                                        Err(err) => {
                                            if err.indicates_epoch_change() {
                                                let mut rng = rand::rngs::OsRng;
                                                let jitter = rng.gen_range(0..RECONFIG_QUIESCENCE_TIME_SEC);
                                                sleep(Duration::from_secs(RECONFIG_QUIESCENCE_TIME_SEC + jitter)).await;

                                                proxy_clone.reconfig().await;
                                            } else {
                                                error!("Retry due to error: {}", err);
                                                metrics_cloned.num_error.with_label_values(&[&payload.get_workload_type().to_string(), err.as_ref()]).inc();
                                            }
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
                                    BenchDriver::update_progress(*start_time, run_duration, progress.clone());
                                    if progress.is_finished() {
                                        break;
                                    }
                                }
                                NextOp::Response(Some((latency, new_payload))) => {
                                    num_success += 1;
                                    num_in_flight -= 1;
                                    free_pool.push(new_payload);
                                    latency_histogram.record(latency.as_millis().try_into().unwrap()).unwrap();
                                    BenchDriver::update_progress(*start_time, run_duration, progress.clone());
                                    if progress.is_finished() {
                                        break;
                                    }
                                }
                                NextOp::Response(None) => {
                                    // num_in_flight -= 1;
                                    unreachable!();
                                }
                            }
                        }
                    }
                }
                // send stats one last time
                if tx_cloned
                    .try_send(Stats {
                        id: i as usize,
                        num_no_gas,
                        num_in_flight,
                        num_submitted,
                        bench_stats: BenchmarkStats {
                            duration: stat_start_time.elapsed(),
                            num_error,
                            num_success,
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

        let stat_task = tokio::spawn(async move {
            let mut benchmark_stat = BenchmarkStats {
                duration: Duration::ZERO,
                num_error: 0,
                num_success: 0,
                latency_ms: HistogramWrapper {
                    histogram: hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap(),
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
                let mut num_success: u64 = 0;
                let mut num_error: u64 = 0;
                let mut latency_histogram =
                    hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
                let mut num_in_flight: u64 = 0;
                let mut num_submitted: u64 = 0;
                let mut num_no_gas = 0;
                for (_, v) in stat_collection.iter() {
                    total_qps +=
                        v.bench_stats.num_success as f32 / v.bench_stats.duration.as_secs() as f32;
                    num_success += v.bench_stats.num_success;
                    num_error += v.bench_stats.num_error;
                    num_no_gas += v.num_no_gas;
                    num_submitted += v.num_submitted;
                    num_in_flight += v.num_in_flight;
                    latency_histogram
                        .add(&v.bench_stats.latency_ms.histogram)
                        .unwrap();
                }
                let denom = num_success + num_error;
                let _error_rate = if denom > 0 {
                    num_error as f32 / denom as f32
                } else {
                    0.0
                };
                counter += 1;
                if counter % num_workers == 0 {
                    stat = format!("Throughput = {}, latency_ms(min/p50/p99/max) = {}/{}/{}/{}, num_success = {}, num_error = {}, no_gas = {}, submitted = {}, in_flight = {}", total_qps, latency_histogram.min(), latency_histogram.value_at_quantile(0.5), latency_histogram.value_at_quantile(0.99), latency_histogram.max(), num_success, num_error, num_no_gas, num_submitted, num_in_flight);
                    if show_progress {
                        eprintln!("{}", stat);
                    }
                }
            }
            benchmark_stat
        });
        drop(tx);
        let all_tasks = try_join_all(tasks);
        let _res = tokio::select! {
            _ = ctrl_c() => {
                self.terminate();
                vec![]
            }
            res = all_tasks => res.unwrap().into_iter().collect()
        };
        let benchmark_stat = stat_task.await.unwrap();
        Ok(benchmark_stat)
    }
}
