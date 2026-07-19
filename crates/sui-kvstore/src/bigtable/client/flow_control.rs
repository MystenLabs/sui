// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

pub(crate) struct BatchWriteFlowController {
    enabled: AtomicBool,
    target_qps: AtomicU64,
    next_update_time: Mutex<Instant>,
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
            next_update_time: Mutex::new(now),
            next_free: AsyncMutex::new(now),
            client_name,
            metrics,
        });

        info!("Batch write flow control: rate limiter initialized (disabled) at {DEFAULT_QPS} QPS");
        controller
    }

    pub(crate) async fn acquire(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let qps = self.current_qps_value();
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
        let current_qps = self.current_qps_value();
        let new_qps = (current_qps * capped_factor).clamp(MIN_QPS, MAX_QPS);
        self.try_set_rate(new_qps, period);
    }

    fn try_set_rate(&self, new_qps: f64, period: Duration) {
        let now = Instant::now();
        let mut next_update_time = self
            .next_update_time
            .lock()
            .expect("flow-control update mutex poisoned");
        if now < *next_update_time {
            self.increment_rate_update("rejected");
            return;
        }

        *next_update_time = now + period;
        let old_qps = self.current_qps_value();
        self.target_qps.store(new_qps.to_bits(), Ordering::Relaxed);
        info!(
            old_qps,
            new_qps,
            period_seconds = period.as_secs_f64(),
            "Batch write flow control: target rate updated"
        );
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_target_qps
                .with_label_values(&[&self.client_name])
                .set(new_qps);
        }
        self.increment_rate_update("applied");
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
        let next_update_time = self
            .next_update_time
            .lock()
            .expect("flow-control update mutex poisoned");
        if now <= *next_update_time {
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
        upper.on_response(Some(&rate_limit_info(2.0, DEFAULT_PERIOD)));
        assert!(upper.is_enabled());
        assert_qps(&upper, DEFAULT_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn rejects_updates_within_period_and_applies_at_period_boundary() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
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
}
