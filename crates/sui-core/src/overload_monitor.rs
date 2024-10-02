// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use mysten_metrics::monitored_scope;
use std::cmp::{max, min};
use std::hash::Hasher;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Weak;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use sui_config::node::AuthorityOverloadConfig;
use sui_types::digests::TransactionDigest;
use sui_types::error::SuiError;
use sui_types::error::SuiResult;
use sui_types::fp_bail;
use tokio::time::sleep;
use tracing::{debug, info};
use twox_hash::XxHash64;

#[derive(Default)]
pub struct AuthorityOverloadInfo {
    /// Whether the authority is overloaded.
    pub is_overload: AtomicBool,

    /// The calculated percentage of transactions to drop.
    pub load_shedding_percentage: AtomicU32,
}

impl AuthorityOverloadInfo {
    pub fn set_overload(&self, load_shedding_percentage: u32) {
        self.is_overload.store(true, Ordering::Relaxed);
        self.load_shedding_percentage
            .store(min(load_shedding_percentage, 100), Ordering::Relaxed);
    }

    pub fn clear_overload(&self) {
        self.is_overload.store(false, Ordering::Relaxed);
        self.load_shedding_percentage.store(0, Ordering::Relaxed);
    }
}

const STEADY_OVERLOAD_REDUCTION_PERCENTAGE: u32 = 10;
const EXECUTION_RATE_RATIO_FOR_COMPARISON: f64 = 0.95;
const ADDITIONAL_LOAD_SHEDDING: f64 = 0.02;

// The update interval of the random seed used to determine whether a txn should be rejected.
const SEED_UPDATE_DURATION_SECS: u64 = 30;

// Monitors the overload signals in `authority_state` periodically, and updates its `overload_info`
// when the signals indicates overload.
pub async fn overload_monitor(
    authority_state: Weak<AuthorityState>,
    config: AuthorityOverloadConfig,
) {
    info!("Starting system overload monitor.");

    loop {
        let authority_exist = check_authority_overload(&authority_state, &config);
        if !authority_exist {
            // `authority_state` doesn't exist anymore. Quit overload monitor.
            break;
        }
        sleep(config.overload_monitor_interval).await;
    }

    info!("Shut down system overload monitor.");
}

// Checks authority overload signals, and updates authority's `overload_info`.
// Returns whether the authority state exists.
fn check_authority_overload(
    authority_state: &Weak<AuthorityState>,
    config: &AuthorityOverloadConfig,
) -> bool {
    let _scope = monitored_scope("OverloadMonitor::check_authority_overload");
    let authority_arc = authority_state.upgrade();
    if authority_arc.is_none() {
        // `authority_state` doesn't exist anymore.
        return false;
    }

    let authority = authority_arc.unwrap();
    let queueing_latency = authority
        .metrics
        .execution_queueing_latency
        .latency()
        .unwrap_or_default();
    let txn_ready_rate = authority.metrics.txn_ready_rate_tracker.lock().rate();
    let execution_rate = authority.metrics.execution_rate_tracker.lock().rate();

    debug!(
        "Check authority overload signal, queueing latency {:?}, ready rate {:?}, execution rate {:?}.",
        queueing_latency, txn_ready_rate, execution_rate
    );

    let (is_overload, load_shedding_percentage) = check_overload_signals(
        config,
        authority
            .overload_info
            .load_shedding_percentage
            .load(Ordering::Relaxed),
        queueing_latency,
        txn_ready_rate,
        execution_rate,
    );

    if is_overload {
        authority
            .overload_info
            .set_overload(load_shedding_percentage);
    } else {
        authority.overload_info.clear_overload();
    }

    authority
        .metrics
        .authority_overload_status
        .set(is_overload as i64);
    authority
        .metrics
        .authority_load_shedding_percentage
        .set(load_shedding_percentage as i64);
    true
}

// Calculates the percentage of transactions to drop in order to reduce execution queue.
// Returns the integer percentage between 0 and 100.
fn calculate_load_shedding_percentage(txn_ready_rate: f64, execution_rate: f64) -> u32 {
    // When transaction ready rate is practically 0, we aren't adding more load to the
    // execution driver, so no shedding.
    // TODO: consensus handler or transaction manager can also be overloaded.
    if txn_ready_rate < 1e-10 {
        return 0;
    }

    // Deflate the execution rate to account for the case that execution_rate is close to
    // txn_ready_rate.
    if execution_rate * EXECUTION_RATE_RATIO_FOR_COMPARISON > txn_ready_rate {
        return 0;
    }

    // In order to maintain execution queue length, we need to drop at least (1 - executionRate / readyRate).
    // To reduce the queue length, here we add 10% more transactions to drop.
    (((1.0 - execution_rate * EXECUTION_RATE_RATIO_FOR_COMPARISON / txn_ready_rate)
        + ADDITIONAL_LOAD_SHEDDING)
        .min(1.0)
        * 100.0)
        .round() as u32
}

// Given overload signals (`queueing_latency`, `txn_ready_rate`, `execution_rate`), return whether
// the authority server should enter load shedding mode, and how much percentage of transactions to drop.
// Note that the final load shedding percentage should also take the current load shedding percentage
// into consideration. If we are already shedding 40% load, based on the current txn_ready_rate
// and execution_rate, we need to shed 10% more, the outcome is that we need to shed
// 40% + (1 - 40%) * 10% = 46%.
// When txn_ready_rate is less than execution_rate, we gradually reduce load shedding percentage until
// the queueing latency is back to normal.
fn check_overload_signals(
    config: &AuthorityOverloadConfig,
    current_load_shedding_percentage: u32,
    queueing_latency: Duration,
    txn_ready_rate: f64,
    execution_rate: f64,
) -> (bool, u32) {
    // First, we calculate based on the current `txn_ready_rate` and `execution_rate`,
    // what's the percentage of traffic to shed from `txn_ready_rate`.
    let additional_load_shedding_percentage;
    if queueing_latency > config.execution_queue_latency_hard_limit {
        let calculated_load_shedding_percentage =
            calculate_load_shedding_percentage(txn_ready_rate, execution_rate);

        additional_load_shedding_percentage = if calculated_load_shedding_percentage > 0
            || txn_ready_rate >= config.safe_transaction_ready_rate as f64
        {
            max(
                calculated_load_shedding_percentage,
                config.min_load_shedding_percentage_above_hard_limit,
            )
        } else {
            0
        };
    } else if queueing_latency > config.execution_queue_latency_soft_limit {
        additional_load_shedding_percentage =
            calculate_load_shedding_percentage(txn_ready_rate, execution_rate);
    } else {
        additional_load_shedding_percentage = 0;
    }

    // Next, we calculate the new load shedding percentage.
    let load_shedding_percentage = if additional_load_shedding_percentage > 0 {
        // When we need to shed more load, since the `txn_ready_rate` is already influenced
        // by `current_load_shedding_percentage`, we need to calculate the new load shedding
        // percentage from `current_load_shedding_percentage` and
        // `additional_load_shedding_percentage`.
        current_load_shedding_percentage
            + (100 - current_load_shedding_percentage) * additional_load_shedding_percentage / 100
    } else if txn_ready_rate > config.safe_transaction_ready_rate as f64
        && current_load_shedding_percentage > 10
    {
        // We don't need to shed more load. However, the enqueue rate is still not minimal.
        // We gradually reduce load shedding percentage (10% at a time) to gracefully accept
        // more load.
        current_load_shedding_percentage - STEADY_OVERLOAD_REDUCTION_PERCENTAGE
    } else {
        // The current transaction ready rate is considered very low. Turn off load shedding mode.
        0
    };

    let load_shedding_percentage = min(
        load_shedding_percentage,
        config.max_load_shedding_percentage,
    );
    let overload_status = load_shedding_percentage > 0;
    (overload_status, load_shedding_percentage)
}

// Return true if we should reject the txn with `tx_digest`.
fn should_reject_tx(
    load_shedding_percentage: u32,
    tx_digest: TransactionDigest,
    temporal_seed: u64,
) -> bool {
    // TODO: we also need to add a secret salt (e.g. first consensus commit in the current epoch),
    // to prevent gaming the system.
    let mut hasher = XxHash64::with_seed(temporal_seed);
    hasher.write(tx_digest.inner());
    let value = hasher.finish();
    value % 100 < load_shedding_percentage as u64
}

// Checks if we can accept the transaction with `tx_digest`.
pub fn overload_monitor_accept_tx(
    load_shedding_percentage: u32,
    tx_digest: TransactionDigest,
) -> SuiResult {
    // Derive a random seed from the epoch time for transaction selection. Changing the seed every
    // `SEED_UPDATE_DURATION_SECS` interval allows rejected transaction's retry to have a chance
    // to go through in the future.
    // Also, using the epoch time instead of randomly generating a seed allows that all validators
    // makes the same decision.
    let temporal_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Sui did not exist prior to 1970")
        .as_secs()
        / SEED_UPDATE_DURATION_SECS;

    if should_reject_tx(load_shedding_percentage, tx_digest, temporal_seed) {
        // TODO: using `SEED_UPDATE_DURATION_SECS` is a safe suggestion that the time based seed
        // is definitely different by then. However, a shorter suggestion may be available.
        fp_bail!(SuiError::ValidatorOverloadedRetryAfter {
            retry_after_secs: SEED_UPDATE_DURATION_SECS
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // allow unbounded_channel() since tests are simulating txn manager execution driver interaction.
mod tests {
    use super::*;

    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use rand::{
        rngs::{OsRng, StdRng},
        Rng, SeedableRng,
    };
    use std::sync::Arc;
    use sui_macros::sim_test;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::sync::mpsc::UnboundedReceiver;
    use tokio::sync::mpsc::UnboundedSender;
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;
    use tokio::time::{interval, Instant, MissedTickBehavior};

    #[test]
    pub fn test_authority_overload_info() {
        let overload_info = AuthorityOverloadInfo::default();
        assert!(!overload_info.is_overload.load(Ordering::Relaxed));
        assert_eq!(
            overload_info
                .load_shedding_percentage
                .load(Ordering::Relaxed),
            0
        );

        {
            overload_info.set_overload(20);
            assert!(overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                20
            );
        }

        // Tests that load shedding percentage can't go beyond 100%.
        {
            overload_info.set_overload(110);
            assert!(overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                100
            );
        }

        {
            overload_info.clear_overload();
            assert!(!overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                0
            );
        }
    }

    #[test]
    pub fn test_calculate_load_shedding_ratio() {
        assert_eq!(calculate_load_shedding_percentage(95.0, 100.1), 0);
        assert_eq!(calculate_load_shedding_percentage(95.0, 100.0), 2);
        assert_eq!(calculate_load_shedding_percentage(100.0, 100.0), 7);
        assert_eq!(calculate_load_shedding_percentage(110.0, 100.0), 16);
        assert_eq!(calculate_load_shedding_percentage(180.0, 100.0), 49);
        assert_eq!(calculate_load_shedding_percentage(100.0, 0.0), 100);
        assert_eq!(calculate_load_shedding_percentage(0.0, 1.0), 0);
    }

    #[test]
    pub fn test_check_overload_signals() {
        let config = AuthorityOverloadConfig {
            execution_queue_latency_hard_limit: Duration::from_secs(10),
            execution_queue_latency_soft_limit: Duration::from_secs(1),
            max_load_shedding_percentage: 90,
            ..Default::default()
        };

        // When execution queueing latency is within soft limit, don't start overload protection.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_millis(500), 1000.0, 10.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit and execution rate is higher, don't
        // start overload protection.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(2), 100.0, 120.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit, but not hard limit, start overload
        // protection.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(2), 100.0, 100.0),
            (true, 7)
        );

        // When execution queueing latency hits hard limit, start more aggressive overload
        // protection.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(11), 100.0, 100.0),
            (true, 50)
        );

        // When execution queueing latency hits hard limit and calculated shedding percentage
        // is higher than min_load_shedding_percentage_above_hard_limit.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(11), 240.0, 100.0),
            (true, 62)
        );

        // When execution queueing latency hits hard limit, but transaction ready rate
        // is within safe_transaction_ready_rate, don't start overload protection.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(11), 20.0, 100.0),
            (false, 0)
        );

        // Maximum transactions shed is cap by `max_load_shedding_percentage` config.
        assert_eq!(
            check_overload_signals(&config, 0, Duration::from_secs(11), 100.0, 0.0),
            (true, 90)
        );

        // When the system is already shedding 50% of load, and the current txn ready rate
        // and execution rate require another 20%, the final shedding rate is 60%.
        assert_eq!(
            check_overload_signals(&config, 50, Duration::from_secs(2), 116.0, 100.0),
            (true, 60)
        );

        // Load shedding percentage is gradually reduced when txn ready rate is lower than
        // execution rate.
        assert_eq!(
            check_overload_signals(&config, 90, Duration::from_secs(2), 200.0, 300.0),
            (true, 80)
        );

        // When queueing delay is above hard limit, we shed additional 50% every time.
        assert_eq!(
            check_overload_signals(&config, 50, Duration::from_secs(11), 100.0, 100.0),
            (true, 75)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_check_authority_overload() {
        telemetry_subscribers::init_for_testing();

        let config = AuthorityOverloadConfig {
            safe_transaction_ready_rate: 0,
            ..Default::default()
        };
        let state = TestAuthorityBuilder::new()
            .with_authority_overload_config(config.clone())
            .build()
            .await;

        // Initialize latency reporter.
        for _ in 0..1000 {
            state
                .metrics
                .execution_queueing_latency
                .report(Duration::from_secs(20));
        }

        // Creates a simple case to see if authority state overload_info can be updated
        // correctly by check_authority_overload.
        let authority = Arc::downgrade(&state);
        assert!(check_authority_overload(&authority, &config));
        assert!(state.overload_info.is_overload.load(Ordering::Relaxed));
        assert_eq!(
            state
                .overload_info
                .load_shedding_percentage
                .load(Ordering::Relaxed),
            config.min_load_shedding_percentage_above_hard_limit
        );

        // Checks that check_authority_overload should return false when the input
        // authority state doesn't exist.
        let authority = Arc::downgrade(&state);
        drop(state);
        assert!(!check_authority_overload(&authority, &config));
    }

    // Creates an AuthorityState and starts an overload monitor that monitors its metrics.
    async fn start_overload_monitor() -> (Arc<AuthorityState>, JoinHandle<()>) {
        let overload_config = AuthorityOverloadConfig::default();
        let state = TestAuthorityBuilder::new()
            .with_authority_overload_config(overload_config.clone())
            .build()
            .await;
        let authority_state = Arc::downgrade(&state);
        let monitor_handle = tokio::spawn(async move {
            overload_monitor(authority_state, overload_config).await;
        });
        (state, monitor_handle)
    }

    // Starts a load generator that generates a steady workload, and also allow it to accept
    // burst of request through `burst_rx`.
    // Request tracking is done by the overload monitor inside `authority`.
    fn start_load_generator(
        steady_rate: f64,
        tx: UnboundedSender<Instant>,
        mut burst_rx: UnboundedReceiver<u32>,
        authority: Arc<AuthorityState>,
        enable_load_shedding: bool,
        total_requests_arc: Arc<AtomicU32>,
        dropped_requests_arc: Arc<AtomicU32>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs_f64(1.0 / steady_rate));
            let mut rng = StdRng::from_rng(&mut OsRng).unwrap();
            let mut total_requests: u32 = 0;
            let mut total_dropped_requests: u32 = 0;

            // Helper function to check whether we should send a request.
            let mut do_send =
                |enable_load_shedding: bool, authority: Arc<AuthorityState>| -> bool {
                    if enable_load_shedding {
                        let shedding_percentage = authority
                            .overload_info
                            .load_shedding_percentage
                            .load(Ordering::Relaxed);
                        !(shedding_percentage > 0 && rng.gen_range(0..100) < shedding_percentage)
                    } else {
                        true
                    }
                };

            loop {
                tokio::select! {
                    now = interval.tick() => {
                        total_requests += 1;
                        if do_send(enable_load_shedding, authority.clone()) {
                            if tx.send(now).is_err() {
                                info!("Load generator stopping. Total requests {:?}, total dropped requests {:?}.", total_requests, total_dropped_requests);
                                total_requests_arc.store(total_requests, Ordering::SeqCst);
                                dropped_requests_arc.store(total_dropped_requests, Ordering::SeqCst);
                                return;
                            }
                            authority.metrics.txn_ready_rate_tracker.lock().record();
                        } else {
                            total_dropped_requests += 1;
                        }
                    }
                    Some(burst) = burst_rx.recv() => {
                        let now = Instant::now();
                        total_requests += burst;
                        for _ in 0..burst {
                            if do_send(enable_load_shedding, authority.clone()) {
                                if tx.send(now).is_err() {
                                    info!("Load generator stopping. Total requests {:?}, total dropped requests {:?}.", total_requests, total_dropped_requests);
                                    total_requests_arc.store(total_requests, Ordering::SeqCst);
                                    dropped_requests_arc.store(total_dropped_requests, Ordering::SeqCst);
                                    return;
                                }
                                authority.metrics.txn_ready_rate_tracker.lock().record();
                            } else {
                                total_dropped_requests += 1;
                            }
                        }
                    }
                }
            }
        })
    }

    // Starts a request executor that can consume request based on `execution_rate`.
    // Request tracking is done by the overload monitor inside `authority`.
    fn start_executor(
        execution_rate: f64,
        mut rx: UnboundedReceiver<Instant>,
        mut stop_rx: oneshot::Receiver<()>,
        authority: Arc<AuthorityState>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs_f64(1.0 / execution_rate));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    Some(start_time) = rx.recv() => {
                        authority.metrics.execution_rate_tracker.lock().record();
                        authority.metrics.execution_queueing_latency.report(start_time.elapsed());
                        interval.tick().await;
                    }
                    _ = &mut stop_rx => {
                        info!("Executor stopping");
                        return;
                    }
                }
            }
        })
    }

    // Helper fundtion to periodically print the current overload info.
    async fn sleep_and_print_stats(state: Arc<AuthorityState>, seconds: u32) {
        for _ in 0..seconds {
            info!(
                "Overload: {:?}. Shedding percentage: {:?}. Queue: {:?}, Ready rate: {:?}. Exec rate: {:?}.",
                state.overload_info.is_overload.load(Ordering::Relaxed),
                state
                    .overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                state.metrics.execution_queueing_latency.latency(),
                state.metrics.txn_ready_rate_tracker.lock().rate(),
                state.metrics.execution_rate_tracker.lock().rate(),
            );
            sleep(Duration::from_secs(1)).await;
        }
    }

    // Running a workload with consistent steady `generator_rate` and `executor_rate`.
    // It checks that the dropped requests should in between min_dropping_rate and max_dropping_rate.
    async fn run_consistent_workload_test(
        generator_rate: f64,
        executor_rate: f64,
        min_dropping_rate: f64,
        max_dropping_rate: f64,
    ) {
        let (state, monitor_handle) = start_overload_monitor().await;

        let (tx, rx) = unbounded_channel();
        let (_burst_tx, burst_rx) = unbounded_channel();
        let total_requests = Arc::new(AtomicU32::new(0));
        let dropped_requests = Arc::new(AtomicU32::new(0));
        let load_generator = start_load_generator(
            generator_rate,
            tx.clone(),
            burst_rx,
            state.clone(),
            true,
            total_requests.clone(),
            dropped_requests.clone(),
        );

        let (stop_tx, stop_rx) = oneshot::channel();
        let executor = start_executor(executor_rate, rx, stop_rx, state.clone());

        sleep_and_print_stats(state.clone(), 300).await;

        stop_tx.send(()).unwrap();
        let _ = tokio::join!(load_generator, executor);

        let dropped_ratio = dropped_requests.load(Ordering::SeqCst) as f64
            / total_requests.load(Ordering::SeqCst) as f64;
        assert!(min_dropping_rate <= dropped_ratio);
        assert!(dropped_ratio <= max_dropping_rate);

        monitor_handle.abort();
        let _ = monitor_handle.await;
    }

    // Tests that when request generation rate is slower than execution rate, no requests should be dropped.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_workload_consistent_no_overload() {
        telemetry_subscribers::init_for_testing();
        run_consistent_workload_test(900.0, 1000.0, 0.0, 0.0).await;
    }

    // Tests that when request generation rate is slightly above execution rate, a small portion of
    // requests should be dropped.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_workload_consistent_slightly_overload() {
        telemetry_subscribers::init_for_testing();
        // Dropping rate should be around 15%.
        run_consistent_workload_test(1100.0, 1000.0, 0.05, 0.25).await;
    }

    // Tests that when request generation rate is much higher than execution rate, a large portion of
    // requests should be dropped.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_workload_consistent_overload() {
        telemetry_subscribers::init_for_testing();
        // Dropping rate should be around 70%.
        run_consistent_workload_test(3000.0, 1000.0, 0.6, 0.8).await;
    }

    // Tests that when there is a very short single spike, no request should be dropped.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_workload_single_spike() {
        telemetry_subscribers::init_for_testing();
        let (state, monitor_handle) = start_overload_monitor().await;

        let (tx, rx) = unbounded_channel();
        let (burst_tx, burst_rx) = unbounded_channel();
        let total_requests = Arc::new(AtomicU32::new(0));
        let dropped_requests = Arc::new(AtomicU32::new(0));
        let load_generator = start_load_generator(
            10.0,
            tx.clone(),
            burst_rx,
            state.clone(),
            true,
            total_requests.clone(),
            dropped_requests.clone(),
        );

        let (stop_tx, stop_rx) = oneshot::channel();
        let executor = start_executor(1000.0, rx, stop_rx, state.clone());

        sleep_and_print_stats(state.clone(), 10).await;
        // Send out a burst of 5000 requests.
        burst_tx.send(5000).unwrap();
        sleep_and_print_stats(state.clone(), 20).await;

        stop_tx.send(()).unwrap();
        let _ = tokio::join!(load_generator, executor);

        // No requests should be dropped.
        assert_eq!(dropped_requests.load(Ordering::SeqCst), 0);

        monitor_handle.abort();
        let _ = monitor_handle.await;
    }

    // Tests that when there are regular spikes that keep queueing latency consistently high,
    // overload monitor should kick in and shed load.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_workload_consistent_short_spike() {
        telemetry_subscribers::init_for_testing();
        let (state, monitor_handle) = start_overload_monitor().await;

        let (tx, rx) = unbounded_channel();
        let (burst_tx, burst_rx) = unbounded_channel();
        let total_requests = Arc::new(AtomicU32::new(0));
        let dropped_requests = Arc::new(AtomicU32::new(0));
        let load_generator = start_load_generator(
            10.0,
            tx.clone(),
            burst_rx,
            state.clone(),
            true,
            total_requests.clone(),
            dropped_requests.clone(),
        );

        let (stop_tx, stop_rx) = oneshot::channel();
        let executor = start_executor(1000.0, rx, stop_rx, state.clone());

        sleep_and_print_stats(state.clone(), 15).await;
        for _ in 0..16 {
            // Regularly send out a burst of request.
            burst_tx.send(10000).unwrap();
            sleep_and_print_stats(state.clone(), 5).await;
        }

        stop_tx.send(()).unwrap();
        let _ = tokio::join!(load_generator, executor);
        let dropped_ratio = dropped_requests.load(Ordering::SeqCst) as f64
            / total_requests.load(Ordering::SeqCst) as f64;

        // We should drop about 50% of request because the burst throughput is about 2x of
        // execution rate.
        assert!(0.4 < dropped_ratio);
        assert!(dropped_ratio < 0.6);

        monitor_handle.abort();
        let _ = monitor_handle.await;
    }

    // Tests that the ratio of rejected transactions created randomly matches load shedding percentage in
    // the overload monitor.
    #[test]
    fn test_txn_rejection_rate() {
        for rejection_percentage in 0..=100 {
            let mut reject_count = 0;
            for _ in 0..10000 {
                let digest = TransactionDigest::random();
                if should_reject_tx(rejection_percentage, digest, 28455473) {
                    reject_count += 1;
                }
            }

            debug!(
                "Rejection percentage: {:?}, reject count: {:?}.",
                rejection_percentage, reject_count
            );
            // Give it a 3% fluctuation.
            assert!(rejection_percentage as f32 / 100.0 - 0.03 < reject_count as f32 / 10000.0);
            assert!(reject_count as f32 / 10000.0 < rejection_percentage as f32 / 100.0 + 0.03);
        }
    }

    // Tests that rejected transaction will have a chance to be accepted in the future.
    #[sim_test]
    async fn test_txn_rejection_over_time() {
        let start_time = Instant::now();
        let mut digest = TransactionDigest::random();
        let mut temporal_seed = 1708108277 / SEED_UPDATE_DURATION_SECS;
        let load_shedding_percentage = 50;

        // Find a rejected transaction with 50% rejection rate.
        while !should_reject_tx(load_shedding_percentage, digest, temporal_seed)
            && start_time.elapsed() < Duration::from_secs(30)
        {
            digest = TransactionDigest::random();
        }

        // It should always be rejected using the current temporal_seed.
        for _ in 0..100 {
            assert!(should_reject_tx(
                load_shedding_percentage,
                digest,
                temporal_seed
            ));
        }

        // It will be accepted in the future.
        temporal_seed += 1;
        while should_reject_tx(load_shedding_percentage, digest, temporal_seed)
            && start_time.elapsed() < Duration::from_secs(30)
        {
            temporal_seed += 1;
        }

        // Make sure that the tests can finish within 30 seconds.
        assert!(start_time.elapsed() < Duration::from_secs(30));
    }
}
