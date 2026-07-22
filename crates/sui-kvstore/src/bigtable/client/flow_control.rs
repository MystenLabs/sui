// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adaptive Bigtable batch-write flow control keeps batch traffic within recently exercised
//! capacity. Bigtable's rate hints regulate write throughput, but demand-backed increases can
//! outpace node autoscaling and tablet rebalancing, degrading latency for reads that share the
//! cluster before the server sends a decrease. Decreases apply immediately, while increases
//! require demand near the current target. When `MutateRows` latency rises above its learned
//! healthy baseline, a latency brake lowers admission and pauses upward growth until autoscaling
//! catches up. Sustained idle targets decay toward observed demand so bursts cannot inherit an
//! untested rate.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Instant;
use tonic::Code;
use tonic::Status;
use tracing::info;

use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::RateLimitInfo;

const DEFAULT_QPS: f64 = 10.0;
const DEFAULT_PERIOD: Duration = Duration::from_secs(10);
const MIN_QPS: f64 = 0.1;
const MAX_QPS: f64 = 100_000.0;
const MIN_FACTOR: f64 = 0.7;
const MAX_FACTOR: f64 = 1.3;
const UPWARD_UTILIZATION_THRESHOLD: f64 = 0.8;
const IDLE_UTILIZATION_THRESHOLD: f64 = 0.2;
const IDLE_EVALS_BEFORE_DECAY: u32 = 6;
const IDLE_DECAY_FACTOR: f64 = 0.7;
const MIN_UTILIZATION_WINDOW: Duration = Duration::from_secs(1);
const BRAKE_MIN: f64 = 0.1;
const BRAKE_CUT_FACTOR: f64 = 0.7;
const BRAKE_RELEASE_FACTOR: f64 = 1.05;
const BRAKE_YELLOW_RATIO: f64 = 1.5;
const BRAKE_RED_RATIO: f64 = 3.0;
const BRAKE_RED_ABSOLUTE: Duration = Duration::from_secs(1);
const BASELINE_EWMA_ALPHA: f64 = 0.2;
const BASELINE_STALE_EVALS: u32 = 90;
const BASELINE_STALE_ALPHA: f64 = 0.05;
const MIN_WINDOW_SAMPLES: u64 = 5;

struct UpdateState {
    next_update_time: Instant,
    window_started_at: Instant,
    underutilized_evals: u32,
    baseline_write_latency_micros: Option<f64>,
    non_green_evals: u32,
}

pub(crate) struct BatchWriteFlowController {
    enabled: AtomicBool,
    target_qps: AtomicU64,
    update_state: Mutex<UpdateState>,
    window_requests: AtomicU64,
    brake: AtomicU64,
    window_latency_total_micros: AtomicU64,
    window_latency_samples: AtomicU64,
    next_free: AsyncMutex<Instant>,
    client_name: String,
    metrics: Option<Arc<KvMetrics>>,
}

impl BatchWriteFlowController {
    pub(crate) fn new(client_name: String, metrics: Option<Arc<KvMetrics>>) -> Arc<Self> {
        let now = Instant::now();
        let controller = Arc::new(Self {
            enabled: AtomicBool::new(false),
            target_qps: AtomicU64::new(DEFAULT_QPS.to_bits()),
            update_state: Mutex::new(UpdateState {
                next_update_time: now,
                window_started_at: now,
                underutilized_evals: 0,
                baseline_write_latency_micros: None,
                non_green_evals: 0,
            }),
            window_requests: AtomicU64::new(0),
            brake: AtomicU64::new(1.0f64.to_bits()),
            window_latency_total_micros: AtomicU64::new(0),
            window_latency_samples: AtomicU64::new(0),
            next_free: AsyncMutex::new(now),
            client_name,
            metrics,
        });

        info!("Batch write flow control: rate limiter initialized (disabled) at {DEFAULT_QPS} QPS");
        controller
    }

    pub(crate) async fn acquire(&self) {
        self.window_requests.fetch_add(1, Ordering::Relaxed);
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let qps = (self.current_qps_value() * self.current_brake_value()).max(MIN_QPS);
        let permit_interval = Duration::from_secs_f64(1.0 / qps);
        let wait = {
            let mut next_free = self.next_free.lock().await;
            let now = Instant::now();
            let wait = next_free.saturating_duration_since(now);
            *next_free = (*next_free).max(now) + permit_interval;
            wait
        };

        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_throttle_ms
                .with_label_values(&[&self.client_name])
                .observe(wait.as_secs_f64() * 1_000.0);
        }

        tokio::time::sleep(wait).await;
    }

    pub(crate) fn on_response(&self, info: Option<&RateLimitInfo>) {
        let Some(info) = info.filter(|info| {
            info.factor > 0.0
                && info
                    .period
                    .as_ref()
                    .is_some_and(|period| period.seconds > 0)
        }) else {
            self.try_disable();
            return;
        };

        self.enable();
        let period = Duration::from_secs(
            info.period
                .as_ref()
                .expect("validated rate limit info has a period")
                .seconds as u64,
        );
        self.update_qps(info.factor, period);
    }

    pub(crate) fn on_error(&self, status: &Status) {
        if !matches!(
            status.code(),
            Code::DeadlineExceeded | Code::Unavailable | Code::ResourceExhausted
        ) {
            return;
        }

        self.increment_rate_update("error-signal");
        self.update_qps(MIN_FACTOR, DEFAULT_PERIOD);
    }

    fn update_qps(&self, factor: f64, period: Duration) {
        let capped_factor = factor.clamp(MIN_FACTOR, MAX_FACTOR);
        let now = Instant::now();
        let mut state = self
            .update_state
            .lock()
            .expect("flow-control update mutex poisoned");
        if now < state.next_update_time {
            self.increment_rate_update("rejected");
            return;
        }

        // Each decision consumes one update period, keeping demand windows at least one period
        // long after the first evaluation.
        let requests = self.window_requests.swap(0, Ordering::Relaxed);
        let elapsed = now
            .saturating_duration_since(state.window_started_at)
            .max(MIN_UTILIZATION_WINDOW);
        let demand_qps = requests as f64 / elapsed.as_secs_f64();
        state.window_started_at = now;
        state.next_update_time = now + period;
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_demand_qps
                .with_label_values(&[&self.client_name])
                .set(demand_qps);
        }

        // Load before swapping so sparse traffic remains accumulated until there are enough
        // samples for a meaningful latency window.
        let samples = self.window_latency_samples.load(Ordering::Relaxed);
        if samples >= MIN_WINDOW_SAMPLES {
            let samples = self.window_latency_samples.swap(0, Ordering::Relaxed);
            let total_micros = self.window_latency_total_micros.swap(0, Ordering::Relaxed);
            let window_avg_micros = total_micros as f64 / samples as f64;
            let baseline = state.baseline_write_latency_micros;
            let red = window_avg_micros >= BRAKE_RED_ABSOLUTE.as_micros() as f64
                || baseline.is_some_and(|b| window_avg_micros > BRAKE_RED_RATIO * b);
            let yellow =
                !red && baseline.is_some_and(|b| window_avg_micros > BRAKE_YELLOW_RATIO * b);
            let brake = self.current_brake_value();

            if red {
                state.non_green_evals = state.non_green_evals.saturating_add(1);
                let cut = (brake * BRAKE_CUT_FACTOR).max(BRAKE_MIN);
                self.brake.store(cut.to_bits(), Ordering::Relaxed);
                self.increment_rate_update("brake-cut");
                info!(
                    window_avg_ms = window_avg_micros / 1_000.0,
                    brake = cut,
                    "Batch write flow control: latency brake engaged"
                );
            } else if yellow {
                state.non_green_evals = state.non_green_evals.saturating_add(1);
                if state.non_green_evals >= BASELINE_STALE_EVALS
                    && let Some(baseline) = state.baseline_write_latency_micros.as_mut()
                {
                    *baseline += BASELINE_STALE_ALPHA * (window_avg_micros - *baseline);
                }
            } else {
                state.non_green_evals = 0;
                let baseline = state
                    .baseline_write_latency_micros
                    .get_or_insert(window_avg_micros);
                *baseline += BASELINE_EWMA_ALPHA * (window_avg_micros - *baseline);
                if brake < 1.0 {
                    let released = (brake * BRAKE_RELEASE_FACTOR).min(1.0);
                    self.brake.store(released.to_bits(), Ordering::Relaxed);
                    self.increment_rate_update("brake-release");
                }
            }
            if let Some(metrics) = &self.metrics {
                metrics
                    .kv_bt_flow_control_brake
                    .with_label_values(&[&self.client_name])
                    .set(self.current_brake_value());
                metrics
                    .kv_bt_flow_control_write_latency_ms
                    .with_label_values(&[&self.client_name])
                    .set(window_avg_micros / 1_000.0);
                if let Some(baseline) = state.baseline_write_latency_micros {
                    metrics
                        .kv_bt_flow_control_write_latency_baseline_ms
                        .with_label_values(&[&self.client_name])
                        .set(baseline / 1_000.0);
                }
            }
        }

        let current_qps = self.current_qps_value();
        let utilization = demand_qps / current_qps;

        if capped_factor < 1.0 {
            self.set_rate(
                (current_qps * capped_factor).clamp(MIN_QPS, MAX_QPS),
                "applied",
            );
            return;
        }

        if utilization >= UPWARD_UTILIZATION_THRESHOLD {
            if self.current_brake_value() < 1.0 {
                state.underutilized_evals = 0;
                self.increment_rate_update("braked");
                info!(
                    factor,
                    "Batch write flow control: upward hint frozen while latency brake engaged"
                );
                return;
            }
            state.underutilized_evals = 0;
            self.set_rate(
                (current_qps * capped_factor).clamp(MIN_QPS, MAX_QPS),
                "applied",
            );
            return;
        }

        if utilization < IDLE_UTILIZATION_THRESHOLD {
            state.underutilized_evals = state.underutilized_evals.saturating_add(1);
        } else if state.underutilized_evals < IDLE_EVALS_BEFORE_DECAY {
            state.underutilized_evals = 0;
        }

        // Once sustained idleness starts decay, continue toward the demand floor through the
        // middle utilization band. A binding target resets this state in the branch above.
        if state.underutilized_evals >= IDLE_EVALS_BEFORE_DECAY {
            let floor = DEFAULT_QPS.max(demand_qps / UPWARD_UTILIZATION_THRESHOLD);
            let decayed = (current_qps * IDLE_DECAY_FACTOR).max(floor);
            if decayed < current_qps {
                self.set_rate(decayed, "idle-decay");
                return;
            }
        }

        self.increment_rate_update("suppressed");
        info!(
            demand_qps,
            target_qps = current_qps,
            factor,
            "Batch write flow control: upward hint suppressed (target not tested by demand)"
        );
    }

    fn set_rate(&self, new_qps: f64, kind: &'static str) {
        let old_qps = self.current_qps_value();
        self.target_qps.store(new_qps.to_bits(), Ordering::Relaxed);
        info!(
            old_qps,
            new_qps, kind, "Batch write flow control: target rate updated"
        );
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_target_qps
                .with_label_values(&[&self.client_name])
                .set(new_qps);
        }
        self.increment_rate_update(kind);
    }

    fn enable(&self) {
        if !self.enabled.swap(true, Ordering::Relaxed) {
            info!("Batch write flow control: rate limiter enabled");
            if let Some(metrics) = &self.metrics {
                metrics
                    .kv_bt_flow_control_enabled
                    .with_label_values(&[&self.client_name])
                    .set(1);
            }
        }
    }

    fn try_disable(&self) {
        let now = Instant::now();
        let update_state = self
            .update_state
            .lock()
            .expect("flow-control update mutex poisoned");
        if now <= update_state.next_update_time {
            return;
        }

        if self.enabled.swap(false, Ordering::Relaxed) {
            info!("Batch write flow control: rate limiter disabled");
            if let Some(metrics) = &self.metrics {
                metrics
                    .kv_bt_flow_control_enabled
                    .with_label_values(&[&self.client_name])
                    .set(0);
            }
            self.increment_rate_update("disabled");
        }
    }

    fn increment_rate_update(&self, kind: &str) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_rate_updates
                .with_label_values(&[&self.client_name, kind])
                .inc();
        }
    }

    fn current_qps_value(&self) -> f64 {
        f64::from_bits(self.target_qps.load(Ordering::Relaxed))
    }

    fn current_brake_value(&self) -> f64 {
        f64::from_bits(self.brake.load(Ordering::Relaxed))
    }

    pub(crate) fn record_write_latency(&self, elapsed: Duration) {
        self.window_latency_total_micros
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
        self.window_latency_samples.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn current_brake(&self) -> f64 {
        self.current_brake_value()
    }

    #[cfg(test)]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn current_qps(&self) -> f64 {
        self.current_qps_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate_limit_info(factor: f64, period: Duration) -> RateLimitInfo {
        RateLimitInfo {
            period: Some(prost_types::Duration {
                seconds: period.as_secs() as i64,
                nanos: period.subsec_nanos() as i32,
            }),
            factor,
        }
    }

    fn assert_qps(controller: &BatchWriteFlowController, expected: f64) {
        let actual = controller.current_qps();
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected} QPS, got {actual}"
        );
    }

    fn seed_demand(controller: &BatchWriteFlowController, requests: u64) {
        controller
            .window_requests
            .fetch_add(requests, Ordering::Relaxed);
    }

    fn seed_latency(controller: &BatchWriteFlowController, samples: u64, each: Duration) {
        controller
            .window_latency_total_micros
            .fetch_add(samples * each.as_micros() as u64, Ordering::Relaxed);
        controller
            .window_latency_samples
            .fetch_add(samples, Ordering::Relaxed);
    }

    #[tokio::test(start_paused = true)]
    async fn starts_disabled_and_acquire_returns_immediately() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        assert!(!controller.is_enabled());
        assert_qps(&controller, DEFAULT_QPS);

        let before = Instant::now();
        controller.acquire().await;
        assert_eq!(Instant::now(), before);
    }

    #[tokio::test(start_paused = true)]
    async fn clamps_lower_and_upper_factors() {
        let lower = BatchWriteFlowController::new("lower".to_owned(), None);
        lower.on_response(Some(&rate_limit_info(0.3, DEFAULT_PERIOD)));
        assert!(lower.is_enabled());
        assert_qps(&lower, DEFAULT_QPS * MIN_FACTOR);

        let upper = BatchWriteFlowController::new("upper".to_owned(), None);
        seed_demand(&upper, 1_000);
        upper.on_response(Some(&rate_limit_info(2.0, DEFAULT_PERIOD)));
        assert!(upper.is_enabled());
        assert_qps(&upper, DEFAULT_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn rejects_updates_within_period_and_applies_at_period_boundary() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        let first_qps = DEFAULT_QPS * MAX_FACTOR;
        assert_qps(&controller, first_qps);

        controller.on_response(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, first_qps);

        tokio::time::advance(DEFAULT_PERIOD).await;
        controller.on_response(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, first_qps * MIN_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn target_qps_stays_within_floor_and_ceiling() {
        let decreasing = BatchWriteFlowController::new("decreasing".to_owned(), None);
        for _ in 0..32 {
            decreasing.on_response(Some(&rate_limit_info(0.01, DEFAULT_PERIOD)));
            assert!(decreasing.current_qps() >= MIN_QPS);
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_qps(&decreasing, MIN_QPS);

        let increasing = BatchWriteFlowController::new("increasing".to_owned(), None);
        for _ in 0..64 {
            seed_demand(&increasing, 2_000_000);
            increasing.on_response(Some(&rate_limit_info(10.0, DEFAULT_PERIOD)));
            assert!(increasing.current_qps() <= MAX_QPS);
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_qps(&increasing, MAX_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn invalid_hints_disable_after_period_and_valid_hint_reenables() {
        let invalid_hints = [
            None,
            Some(rate_limit_info(0.0, DEFAULT_PERIOD)),
            Some(rate_limit_info(1.0, Duration::ZERO)),
        ];

        for invalid_hint in invalid_hints {
            let controller = BatchWriteFlowController::new("test".to_owned(), None);
            controller.on_response(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
            assert!(controller.is_enabled());
            let retained_qps = controller.current_qps();

            controller.on_response(invalid_hint.as_ref());
            assert!(controller.is_enabled());
            assert_qps(&controller, retained_qps);

            tokio::time::advance(DEFAULT_PERIOD + Duration::from_nanos(1)).await;
            controller.on_response(invalid_hint.as_ref());
            assert!(!controller.is_enabled());
            assert_qps(&controller, retained_qps);

            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            assert!(controller.is_enabled());
            assert_qps(&controller, retained_qps);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn qualifying_errors_cut_rate_without_enabling() {
        for code in [
            Code::DeadlineExceeded,
            Code::Unavailable,
            Code::ResourceExhausted,
        ] {
            let controller = BatchWriteFlowController::new("test".to_owned(), None);
            controller.on_error(&Status::new(code, "transient"));
            assert!(!controller.is_enabled());
            assert_qps(&controller, DEFAULT_QPS * MIN_FACTOR);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn non_qualifying_error_does_not_change_rate() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_error(&Status::not_found("missing"));
        assert!(!controller.is_enabled());
        assert_qps(&controller, DEFAULT_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn upward_hint_suppressed_without_demand_then_applies_with_demand() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert!(controller.is_enabled());
        assert_qps(&controller, DEFAULT_QPS);

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn suppressed_hint_consumes_update_period() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS);

        tokio::time::advance(DEFAULT_PERIOD + Duration::from_nanos(1)).await;
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn sustained_underutilization_decays_target_to_demand_floor() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .target_qps
            .store(5_000.0f64.to_bits(), Ordering::Relaxed);
        controller.enable();
        tokio::time::advance(DEFAULT_PERIOD).await;

        for _ in 0..16 {
            seed_demand(&controller, 800);
            controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_qps(&controller, 100.0);

        seed_demand(&controller, 800);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, 130.0);
    }

    #[tokio::test(start_paused = true)]
    async fn decay_without_demand_floors_at_default_qps() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .target_qps
            .store(100.0f64.to_bits(), Ordering::Relaxed);
        controller.enable();
        tokio::time::advance(DEFAULT_PERIOD).await;

        for _ in 0..12 {
            controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_qps(&controller, DEFAULT_QPS);

        for _ in 0..3 {
            controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
            assert_qps(&controller, DEFAULT_QPS);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn mid_band_utilization_resets_idle_counter() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .target_qps
            .store(100.0f64.to_bits(), Ordering::Relaxed);
        controller.enable();
        tokio::time::advance(DEFAULT_PERIOD).await;

        for _ in 0..3 {
            seed_demand(&controller, 100);
            controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        seed_demand(&controller, 500);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        tokio::time::advance(DEFAULT_PERIOD).await;

        for _ in 0..5 {
            seed_demand(&controller, 100);
            controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_qps(&controller, 100.0);

        seed_demand(&controller, 100);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, 70.0);
    }

    #[tokio::test(start_paused = true)]
    async fn decrease_applies_when_underutilized() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .target_qps
            .store(100.0f64.to_bits(), Ordering::Relaxed);
        controller.enable();
        tokio::time::advance(DEFAULT_PERIOD).await;

        controller.on_response(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, 70.0);
    }

    #[tokio::test(start_paused = true)]
    async fn spaces_permits_without_burst_credit() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .target_qps
            .store(2.0f64.to_bits(), Ordering::Relaxed);
        controller.enable();

        controller.acquire().await;
        let first_permit = Instant::now();
        controller.acquire().await;
        let second_permit = Instant::now();

        assert_eq!(
            second_permit.duration_since(first_permit),
            Duration::from_millis(500)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn brake_engages_on_red_window_and_throttles_admission() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        seed_latency(&controller, 10, Duration::from_millis(200));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);

        controller
            .target_qps
            .store(2.0f64.to_bits(), Ordering::Relaxed);
        controller.acquire().await;
        let first_permit = Instant::now();
        controller.acquire().await;
        let second_permit = Instant::now();

        let permit_spacing = second_permit.duration_since(first_permit);
        let expected_spacing = Duration::from_secs_f64(1.0 / (2.0 * BRAKE_CUT_FACTOR));
        assert!(
            permit_spacing >= expected_spacing
                && permit_spacing < expected_spacing + Duration::from_millis(1),
            "expected permit spacing within 1 ms above {expected_spacing:?}, got {permit_spacing:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn yellow_window_holds_brake_and_baseline() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        seed_latency(&controller, 10, Duration::from_millis(40));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert_eq!(controller.current_brake(), 1.0);
        assert_eq!(
            controller
                .update_state
                .lock()
                .expect("flow-control update mutex poisoned")
                .baseline_write_latency_micros,
            Some(20_000.0)
        );

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_latency(&controller, 10, Duration::from_millis(200));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);
    }

    #[tokio::test(start_paused = true)]
    async fn brake_releases_slowly_on_green() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(200));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        let mut expected_brake = BRAKE_CUT_FACTOR * BRAKE_CUT_FACTOR;
        assert!((controller.current_brake() - expected_brake).abs() < f64::EPSILON);

        for green_eval in 1..=15 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            expected_brake = (expected_brake * BRAKE_RELEASE_FACTOR).min(1.0);
            assert!((controller.current_brake() - expected_brake).abs() < f64::EPSILON);
            if green_eval < 15 {
                assert!(controller.current_brake() < 1.0);
            }
            tokio::time::advance(DEFAULT_PERIOD).await;
        }
        assert_eq!(controller.current_brake(), 1.0);
    }

    #[tokio::test(start_paused = true)]
    async fn upward_hint_frozen_while_braked() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        seed_latency(&controller, 10, Duration::from_millis(200));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!(controller.current_brake() < 1.0);

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS);

        for _ in 0..15 {
            tokio::time::advance(DEFAULT_PERIOD).await;
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        }
        assert_eq!(controller.current_brake(), 1.0);
        assert_qps(&controller, DEFAULT_QPS);

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn absolute_cap_trips_red_without_baseline() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        seed_latency(&controller, 10, Duration::from_millis(1_500));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);
        assert!(
            controller
                .update_state
                .lock()
                .expect("flow-control update mutex poisoned")
                .baseline_write_latency_micros
                .is_none()
        );

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_latency(&controller, 10, Duration::from_millis(20));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert_eq!(
            controller
                .update_state
                .lock()
                .expect("flow-control update mutex poisoned")
                .baseline_write_latency_micros,
            Some(20_000.0)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sparse_windows_accumulate_until_sample_floor() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        seed_latency(&controller, 3, Duration::from_secs(2));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert_eq!(controller.current_brake(), 1.0);
        assert_eq!(controller.window_latency_samples.load(Ordering::Relaxed), 3);

        tokio::time::advance(DEFAULT_PERIOD).await;
        seed_latency(&controller, 2, Duration::from_secs(2));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);
        assert_eq!(controller.window_latency_samples.load(Ordering::Relaxed), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn server_decrease_applies_while_braked() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        seed_latency(&controller, 10, Duration::from_millis(1_500));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);

        tokio::time::advance(DEFAULT_PERIOD).await;
        controller.on_response(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        assert_qps(&controller, DEFAULT_QPS * MIN_FACTOR);
        assert!((controller.current_brake() - BRAKE_CUT_FACTOR).abs() < f64::EPSILON);
    }

    #[tokio::test(start_paused = true)]
    async fn stale_yellow_baseline_drifts_until_green() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);

        for _ in 0..2 {
            seed_latency(&controller, 10, Duration::from_millis(20));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            tokio::time::advance(DEFAULT_PERIOD).await;
        }

        seed_latency(&controller, 10, Duration::from_millis(200));
        seed_demand(&controller, 1_000);
        controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        let cut_brake = controller.current_brake();

        let mut reached_green = false;
        for _ in 0..BASELINE_STALE_EVALS + 10 {
            tokio::time::advance(DEFAULT_PERIOD).await;
            seed_latency(&controller, 10, Duration::from_millis(40));
            seed_demand(&controller, 1_000);
            controller.on_response(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
            if controller.current_brake() > cut_brake {
                reached_green = true;
                break;
            }
        }

        assert!(reached_green, "stale yellow baseline never reached green");
        assert!(
            (controller.current_brake() - cut_brake * BRAKE_RELEASE_FACTOR).abs() < f64::EPSILON
        );
    }
}
