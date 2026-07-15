// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use std::time::Instant;

use arc_swap::ArcSwap;
use governor::Quota;
use governor::RateLimiter as GovRateLimiter;
use tonic::Code;
use tracing::info;

use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::RateLimitInfo;

// Mirrors java-bigtable's RateLimitingServerStreamingCallable write flow-control
// constants. MIN_QPS intentionally differs: Java uses 0.1, but governor's
// Quota::per_second takes a NonZeroU32, so this implementation floors at 1 QPS.
const DEFAULT_QPS: f64 = 10.0;
const DEFAULT_PERIOD: Duration = Duration::from_secs(10);
const MIN_QPS: f64 = 1.0;
const MAX_QPS: f64 = 100_000.0;
const MIN_FACTOR: f64 = 0.7;
const MAX_FACTOR: f64 = 1.3;

pub(crate) struct WriteRateLimiter {
    enabled: AtomicBool,
    rate: Mutex<f64>,
    limiter: ArcSwap<governor::DefaultDirectRateLimiter>,
    next_update: Mutex<Instant>,
    metrics: Option<Arc<KvMetrics>>,
    client_name: String,
}

impl WriteRateLimiter {
    pub(crate) fn new(metrics: Option<Arc<KvMetrics>>, client_name: String) -> Arc<Self> {
        let limiter = Arc::new(Self {
            enabled: AtomicBool::new(false),
            rate: Mutex::new(DEFAULT_QPS),
            limiter: ArcSwap::from_pointee(GovRateLimiter::direct(quota_for(DEFAULT_QPS))),
            next_update: Mutex::new(Instant::now()),
            metrics,
            client_name,
        });
        limiter.set_qps_metric(DEFAULT_QPS);
        limiter.set_enabled_metric(false);
        limiter
    }

    pub(crate) async fn acquire(&self) {
        if !self.enabled.load(Relaxed) {
            return;
        }
        self.limiter.load_full().until_ready().await;
    }

    pub(crate) fn on_response_part(&self, info: Option<&RateLimitInfo>) {
        let Some(info) = info else {
            self.try_disable_at(Instant::now());
            return;
        };
        let Some(period) = info.period else {
            self.try_disable_at(Instant::now());
            return;
        };
        let valid = info.factor > 0.0 && period.seconds > 0;
        if !valid {
            self.try_disable_at(Instant::now());
            return;
        }

        self.inc_hints_metric();
        self.enable();
        self.update_qps_at(
            info.factor,
            Duration::from_secs(period.seconds as u64),
            Instant::now(),
        );
    }

    pub(crate) fn on_error(&self, status: &tonic::Status) {
        if matches!(
            status.code(),
            Code::DeadlineExceeded | Code::Unavailable | Code::ResourceExhausted
        ) {
            self.update_qps_at(MIN_FACTOR, DEFAULT_PERIOD, Instant::now());
        }
    }

    fn enable(&self) {
        self.enabled.store(true, Relaxed);
        self.set_enabled_metric(true);
    }

    fn try_disable_at(&self, now: Instant) {
        let next_update = *self
            .next_update
            .lock()
            .expect("write rate limiter next_update mutex poisoned");
        if now > next_update {
            self.enabled.store(false, Relaxed);
            self.set_enabled_metric(false);
        }
    }

    fn update_qps_at(&self, factor: f64, period: Duration, now: Instant) {
        let capped = factor.clamp(MIN_FACTOR, MAX_FACTOR);
        let (old, new) = {
            let mut next_update = self
                .next_update
                .lock()
                .expect("write rate limiter next_update mutex poisoned");
            if now < *next_update {
                return;
            }
            // Guard against a pathological/hostile hint period overflowing the
            // Instant: treat an un-representable deadline as a no-op rather than
            // panicking under the locks (which would poison them and, via the
            // shared Arc, wedge every future write).
            let Some(next) = now.checked_add(period) else {
                return;
            };

            let mut rate = self
                .rate
                .lock()
                .expect("write rate limiter rate mutex poisoned");
            *next_update = next;
            let old = *rate;
            *rate = (*rate * capped).clamp(MIN_QPS, MAX_QPS);
            let new = *rate;
            self.limiter
                .store(Arc::new(GovRateLimiter::direct(quota_for(new))));
            (old, new)
        };
        // Log/metric after releasing the mutexes so a slow (or panicking)
        // tracing subscriber cannot extend the critical section or poison locks.
        info!(
            client = %self.client_name,
            old_qps = old,
            new_qps = new,
            factor,
            capped_factor = capped,
            ?period,
            "updated BigTable write flow-control QPS"
        );
        self.set_qps_metric(new);
    }

    fn set_qps_metric(&self, qps: f64) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_write_flow_control_qps
                .with_label_values(&[&self.client_name])
                .set(qps);
        }
    }

    fn set_enabled_metric(&self, enabled: bool) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_write_flow_control_enabled
                .with_label_values(&[&self.client_name])
                .set(i64::from(enabled));
        }
    }

    fn inc_hints_metric(&self) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_write_flow_control_hints
                .with_label_values(&[&self.client_name])
                .inc();
        }
    }

    #[cfg(test)]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.load(Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn current_qps(&self) -> f64 {
        *self
            .rate
            .lock()
            .expect("write rate limiter rate mutex poisoned")
    }
}

fn quota_for(rate: f64) -> Quota {
    Quota::per_second(NonZeroU32::new((rate.round() as u32).max(1)).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> Arc<WriteRateLimiter> {
        WriteRateLimiter::new(None, "test".to_string())
    }

    fn hint(factor: f64, seconds: i64) -> RateLimitInfo {
        RateLimitInfo {
            period: Some(prost_types::Duration { seconds, nanos: 0 }),
            factor,
        }
    }

    fn assert_qps_approx(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected qps {expected}, got {actual}"
        );
    }

    #[test]
    fn valid_hint_enables_and_clamps_factor_up() {
        let limiter = limiter();
        let t0 = Instant::now();

        limiter.enable();
        limiter.update_qps_at(1.5, Duration::from_secs(10), t0);

        assert!(limiter.is_enabled());
        assert_eq!(limiter.current_qps(), 13.0);
    }

    #[test]
    fn second_update_within_period_is_no_op() {
        let limiter = limiter();
        let t0 = Instant::now();

        limiter.enable();
        limiter.update_qps_at(1.5, Duration::from_secs(10), t0);
        limiter.update_qps_at(0.5, Duration::from_secs(10), t0 + Duration::from_secs(1));

        assert_eq!(limiter.current_qps(), 13.0);
    }

    #[test]
    fn update_after_period_clamps_low_factor_down() {
        let limiter = limiter();
        let t0 = Instant::now();

        limiter.enable();
        limiter.update_qps_at(1.5, Duration::from_secs(10), t0);
        limiter.update_qps_at(0.5, Duration::from_secs(10), t0 + Duration::from_secs(11));

        assert_qps_approx(limiter.current_qps(), 9.1);
    }

    #[test]
    fn rate_clamps_at_min_and_max_qps() {
        let down = limiter();
        let t0 = Instant::now();
        for k in 0..=20 {
            down.update_qps_at(0.7, Duration::from_secs(1), t0 + Duration::from_secs(k));
            assert!(down.current_qps() >= MIN_QPS);
        }
        assert_eq!(down.current_qps(), MIN_QPS);

        let up = limiter();
        let t0 = Instant::now();
        for k in 0..=50 {
            up.update_qps_at(1.3, Duration::from_secs(1), t0 + Duration::from_secs(k));
            assert!(up.current_qps() <= MAX_QPS);
        }
        assert_eq!(up.current_qps(), MAX_QPS);
    }

    #[test]
    fn invalid_or_absent_hints_disable_only_after_next_update() {
        let limiter = limiter();
        let t0 = Instant::now();

        limiter.enable();
        limiter.update_qps_at(1.0, Duration::from_secs(10), t0);

        limiter.try_disable_at(t0 + Duration::from_secs(1));
        assert!(limiter.is_enabled());

        limiter.try_disable_at(t0 + Duration::from_secs(11));
        assert!(!limiter.is_enabled());
        assert_eq!(limiter.current_qps(), 10.0);

        let valid_hint = hint(1.0, 10);
        limiter.on_response_part(Some(&valid_hint));
        assert!(limiter.is_enabled());
        assert_eq!(limiter.current_qps(), 10.0);

        let absent = WriteRateLimiter::new(None, "test".to_string());
        absent.on_response_part(None);
        assert!(!absent.is_enabled());

        let zero_factor = WriteRateLimiter::new(None, "test".to_string());
        let zero_factor_hint = hint(0.0, 10);
        zero_factor.on_response_part(Some(&zero_factor_hint));
        assert!(!zero_factor.is_enabled());

        let zero_period = WriteRateLimiter::new(None, "test".to_string());
        let zero_period_hint = hint(1.0, 0);
        zero_period.on_response_part(Some(&zero_period_hint));
        assert!(!zero_period.is_enabled());
    }

    #[test]
    fn throttling_errors_back_off_without_enabling() {
        for status in [
            tonic::Status::unavailable("x"),
            tonic::Status::deadline_exceeded("x"),
            tonic::Status::resource_exhausted("x"),
        ] {
            let limiter = limiter();

            limiter.on_error(&status);

            assert_eq!(limiter.current_qps(), 7.0);
            assert!(!limiter.is_enabled());
        }

        let limiter = limiter();
        limiter.on_error(&tonic::Status::internal("x"));

        assert_eq!(limiter.current_qps(), 10.0);
        assert!(!limiter.is_enabled());
    }

    #[test]
    fn overflowing_hint_period_is_a_noop_not_a_panic() {
        let limiter = limiter();
        let t0 = Instant::now();

        limiter.enable();
        limiter.update_qps_at(1.2, Duration::from_secs(10), t0);
        assert_eq!(limiter.current_qps(), 12.0);

        limiter.update_qps_at(1.3, Duration::MAX, t0 + Duration::from_secs(11));

        assert_eq!(limiter.current_qps(), 12.0);
        assert!(limiter.is_enabled());

        limiter.update_qps_at(1.1, Duration::from_secs(10), t0 + Duration::from_secs(12));
        assert_qps_approx(limiter.current_qps(), 13.2);
    }

    #[tokio::test]
    async fn acquire_returns_immediately_when_disabled() {
        let limiter = limiter();

        assert!(
            tokio::time::timeout(Duration::from_millis(50), limiter.acquire())
                .await
                .is_ok()
        );
    }
}
