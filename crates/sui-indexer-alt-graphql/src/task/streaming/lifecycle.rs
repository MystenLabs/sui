// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::Histogram;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::IntGauge;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::broadcast;
use tokio::time::Instant;

use crate::error::RpcError;
use crate::error::resource_exhausted;
use crate::metrics::SubscriptionMetrics;

/// Server-wide cap on the number of concurrent subscriptions, as a pool of admission slots.
/// Registered once in the schema data; each admitted subscription's guard holds one slot for its
/// lifetime, and a subscription opened while all slots are taken is rejected.
#[derive(Clone)]
pub(crate) struct SubscriberLimit(Arc<Semaphore>);

impl SubscriberLimit {
    pub(crate) fn new(max_subscribers: usize) -> Self {
        Self(Arc::new(Semaphore::new(max_subscribers)))
    }

    /// Claim a slot, or `None` when all are taken. The slot is released when the returned permit is
    /// dropped.
    fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        self.0.clone().try_acquire_owned().ok()
    }
}

/// Why a subscription ended, used to label `subscription_terminations`.
#[derive(Clone, Copy)]
pub(crate) enum SubscriptionTerminationReason {
    ClientClosed,
    Lagged,
    Error,
    Shutdown,
    BackfillError,
    /// A defensive disconnect for an invariant that should never fire; a nonzero count is a bug.
    UnexpectedGap,
}

impl SubscriptionTerminationReason {
    fn metric_label(self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed",
            Self::Lagged => "lagged",
            Self::Error => "error",
            Self::Shutdown => "shutdown",
            Self::BackfillError => "backfill_error",
            Self::UnexpectedGap => "unexpected_gap",
        }
    }

    pub(crate) fn from_recv_error(e: &broadcast::error::RecvError) -> Self {
        match e {
            broadcast::error::RecvError::Lagged(_) => Self::Lagged,
            broadcast::error::RecvError::Closed => Self::Shutdown,
        }
    }
}

/// Tracks one subscription for the lifetime of its stream: on construction bumps the
/// active-subscription gauge and the opened counter, and on drop decrements the gauge, records the
/// lifetime, and records a termination. Call `terminate` to record why it ended and consume the
/// guard; a guard dropped without that call (for example when the client disconnects and the stream
/// future is cancelled) records `client_closed`.
pub(crate) struct SubscriptionLifecycleGuard {
    active_subscriptions: IntGauge,
    subscription_terminations: IntCounterVec,
    subscription_duration: Histogram,
    live_delivered: IntCounter,
    backfill_delivered: IntCounter,
    live_delivery_lag: Histogram,
    subscription_type: &'static str,
    termination_reason: Option<SubscriptionTerminationReason>,
    started_at: Instant,
    /// The concurrent-subscription slot this subscription holds; released when the guard drops.
    _permit: OwnedSemaphorePermit,
}

impl SubscriptionLifecycleGuard {
    /// Admit a subscription: claim a concurrency slot from `subscriber_limit`, or return an
    /// at-capacity error (counting a rejection) when the server is already full. The returned guard
    /// holds the slot and all per-subscriber metric handles for the subscription's lifetime.
    pub(crate) fn new(
        subscription_type: &'static str,
        metrics: &SubscriptionMetrics,
        subscriber_limit: &SubscriberLimit,
    ) -> Result<Self, RpcError> {
        let label = subscription_type;
        let Some(permit) = subscriber_limit.try_acquire() else {
            metrics
                .subscriptions_rejected
                .with_label_values(&[label, "at_capacity"])
                .inc();
            return Err(resource_exhausted(anyhow::anyhow!(
                "The server is at its concurrent-subscription capacity; retry later"
            )));
        };
        let active_subscriptions = metrics.active_subscriptions.with_label_values(&[label]);
        active_subscriptions.inc();
        metrics
            .subscriptions_opened
            .with_label_values(&[label])
            .inc();
        Ok(Self {
            active_subscriptions,
            subscription_terminations: metrics.subscription_terminations.clone(),
            subscription_duration: metrics.subscription_duration.with_label_values(&[label]),
            live_delivered: metrics
                .payloads_delivered
                .with_label_values(&[label, "live"]),
            backfill_delivered: metrics
                .payloads_delivered
                .with_label_values(&[label, "backfill"]),
            live_delivery_lag: metrics
                .live_payload_delivery_checkpoint_timestamp_lag
                .with_label_values(&[label]),
            subscription_type,
            termination_reason: None,
            started_at: Instant::now(),
            _permit: permit,
        })
    }

    /// Record why the subscription ended, consuming the guard so it cannot be used afterward.
    ///
    /// The guard's `Drop` at the end of this call is what records the termination, with the reason
    /// set here. A guard dropped without calling this means the client closed the connection.
    pub(crate) fn terminate(mut self, reason: SubscriptionTerminationReason) {
        self.termination_reason = Some(reason);
    }

    /// Record delivery of one live payload: bump the live counter and observe its freshness (now
    /// minus its checkpoint's timestamp). One call per payload, so a checkpoint with N matching
    /// transactions/events records N samples that climb as the throttle paces them out.
    pub(crate) fn record_delivered(&self, checkpoint_timestamp_ms: u64) {
        self.live_delivered.inc();
        let lag_ms = chrono::Utc::now().timestamp_millis() - checkpoint_timestamp_ms as i64;
        self.live_delivery_lag
            .observe(lag_ms.max(0) as f64 / 1000.0);
    }

    /// Record delivery of one backfilled payload. Counted under the `backfill` phase and without a
    /// freshness sample: backfill replays historical checkpoints, so their timestamp lag is catch-up
    /// distance, not delivery staleness.
    pub(crate) fn record_backfill_delivered(&self) {
        self.backfill_delivered.inc();
    }
}

impl Drop for SubscriptionLifecycleGuard {
    fn drop(&mut self) {
        self.active_subscriptions.dec();
        self.subscription_duration
            .observe(self.started_at.elapsed().as_secs_f64());
        // No recorded reason means the stream was dropped without a terminal event: the client left.
        let reason = self
            .termination_reason
            .unwrap_or(SubscriptionTerminationReason::ClientClosed);
        self.subscription_terminations
            .with_label_values(&[self.subscription_type, reason.metric_label()])
            .inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guard dropped without a set reason bumps then clears the active gauge, counts one opened,
    /// records a lifetime sample, and attributes the termination to `client_closed`.
    #[tokio::test]
    async fn drop_without_reason_is_client_closed() {
        let metrics = SubscriptionMetrics::new_for_test();
        let label = "transactions";

        {
            let _guard = SubscriptionLifecycleGuard::new(
                "transactions",
                &metrics,
                &SubscriberLimit::new(10),
            )
            .unwrap();
            assert_eq!(active(&metrics, label), 1);
            assert_eq!(opened(&metrics, label), 1);
        }

        assert_eq!(active(&metrics, label), 0);
        assert_eq!(duration_samples(&metrics, label), 1);
        assert_eq!(terminations(&metrics, label, "client_closed"), 1);
    }

    /// A set reason overrides the default and labels the termination accordingly.
    #[tokio::test]
    async fn set_reason_is_recorded() {
        let metrics = SubscriptionMetrics::new_for_test();
        let label = "events";

        {
            let guard =
                SubscriptionLifecycleGuard::new("events", &metrics, &SubscriberLimit::new(10))
                    .unwrap();
            guard.terminate(SubscriptionTerminationReason::Error);
        }

        assert_eq!(active(&metrics, label), 0);
        assert_eq!(terminations(&metrics, label, "error"), 1);
        assert_eq!(terminations(&metrics, label, "client_closed"), 0);
    }

    /// Live deliveries bump the live counter and one lag sample each; backfill deliveries bump only
    /// the backfill counter (no lag sample).
    #[tokio::test]
    async fn record_delivered_counts_by_phase() {
        let metrics = SubscriptionMetrics::new_for_test();
        let label = "transactions";
        let guard =
            SubscriptionLifecycleGuard::new("transactions", &metrics, &SubscriberLimit::new(10))
                .unwrap();

        guard.record_delivered(0);
        guard.record_delivered(0);
        guard.record_backfill_delivered();

        assert_eq!(delivered(&metrics, label, "live"), 2);
        assert_eq!(delivered(&metrics, label, "backfill"), 1);
        assert_eq!(delivery_lag_samples(&metrics, label), 2);
    }

    /// A subscription opened at capacity is refused (`None`) and counted as rejected; freeing the
    /// held slot lets the next one in.
    #[tokio::test]
    async fn rejects_at_capacity() {
        let metrics = SubscriptionMetrics::new_for_test();
        let label = "transactions";
        let subscriber_limit = SubscriberLimit::new(1);

        let first = SubscriptionLifecycleGuard::new("transactions", &metrics, &subscriber_limit);
        assert!(first.is_ok(), "first admitted");
        assert!(
            SubscriptionLifecycleGuard::new("transactions", &metrics, &subscriber_limit).is_err(),
            "second refused at capacity",
        );
        assert_eq!(rejected(&metrics, label, "at_capacity"), 1);

        // Dropping the first frees its slot, admitting the next.
        drop(first);
        assert!(
            SubscriptionLifecycleGuard::new("transactions", &metrics, &subscriber_limit).is_ok(),
            "slot freed on drop",
        );
    }

    fn active(m: &SubscriptionMetrics, label: &str) -> i64 {
        m.active_subscriptions.with_label_values(&[label]).get()
    }

    fn opened(m: &SubscriptionMetrics, label: &str) -> u64 {
        m.subscriptions_opened.with_label_values(&[label]).get()
    }

    fn rejected(m: &SubscriptionMetrics, label: &str, reason: &str) -> u64 {
        m.subscriptions_rejected
            .with_label_values(&[label, reason])
            .get()
    }

    fn terminations(m: &SubscriptionMetrics, label: &str, reason: &str) -> u64 {
        m.subscription_terminations
            .with_label_values(&[label, reason])
            .get()
    }

    fn duration_samples(m: &SubscriptionMetrics, label: &str) -> u64 {
        m.subscription_duration
            .with_label_values(&[label])
            .get_sample_count()
    }

    fn delivered(m: &SubscriptionMetrics, label: &str, phase: &str) -> u64 {
        m.payloads_delivered
            .with_label_values(&[label, phase])
            .get()
    }

    fn delivery_lag_samples(m: &SubscriptionMetrics, label: &str) -> u64 {
        m.live_payload_delivery_checkpoint_timestamp_lag
            .with_label_values(&[label])
            .get_sample_count()
    }
}
