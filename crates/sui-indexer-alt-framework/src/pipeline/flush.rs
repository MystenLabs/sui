// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pieces shared by the concurrent and sequential collectors when deciding when to cut a batch and
//! hand it to the committer (see [`FlushPolicy`]).

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use tokio::sync::Notify;
use tokio::sync::futures::Notified;
use tokio::time::Interval;
use tokio::time::MissedTickBehavior;
use tokio::time::interval;

use crate::pipeline::CommitterConfig;
use crate::pipeline::FlushPolicy;

/// Counts the batches a collector has handed to its committer that have not finished being
/// written yet.
#[derive(Default)]
pub(crate) struct InFlightWrites {
    count: AtomicUsize,

    /// Woken every time `count` drops to zero.
    drained: Notify,
}

/// Marks one batch as handed to the committer and not yet written. It travels with the batch, and
/// dropping it marks the write as finished, so a batch that is discarded without being written
/// (e.g. during shutdown) cannot leave the count stuck above zero.
pub(crate) struct InFlightWrite(Arc<InFlightWrites>);

/// Why a collector started handing pending rows to its committer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlushReason {
    /// At least `min_eager_rows` rows were pending.
    Threshold,

    /// [`FlushPolicy::WhenIdle`]: rows were pending and no earlier batch was still being written.
    Idle,

    /// [`FlushPolicy::Interval`]: the collect interval elapsed.
    Interval,

    /// The processor closed its channel, so everything still pending is written out.
    Shutdown,
}

impl InFlightWrites {
    /// Count one more batch as in flight, until the returned marker is dropped.
    pub(crate) fn start(self: &Arc<Self>) -> InFlightWrite {
        self.count.fetch_add(1, Ordering::SeqCst);
        InFlightWrite(self.clone())
    }

    /// Whether every batch handed to the committer has finished being written.
    pub(crate) fn is_empty(&self) -> bool {
        self.count.load(Ordering::SeqCst) == 0
    }

    /// A future that resolves the next time the count drops to zero. It observes every drop that
    /// happens after this call, even before it is first polled, so create it *before* checking
    /// [`Self::is_empty`]: a write that finishes between the check and the wait still wakes it.
    pub(crate) fn drained(&self) -> Notified<'_> {
        self.drained.notified()
    }
}

impl FlushReason {
    /// Value of the `reason` label on the `total_collector_flushes` metric.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FlushReason::Threshold => "threshold",
            FlushReason::Idle => "idle",
            FlushReason::Interval => "interval",
            FlushReason::Shutdown => "shutdown",
        }
    }
}

impl Drop for InFlightWrite {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.drained.notify_waiters();
        }
    }
}

/// The timer that drives [`FlushPolicy::Interval`] flushes. `None` under
/// [`FlushPolicy::WhenIdle`], which does not use a timer.
pub(crate) fn collect_timer(config: &CommitterConfig) -> Option<Interval> {
    match config.flush_policy {
        FlushPolicy::Interval => {
            let mut timer = interval(config.collect_interval());
            timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
            Some(timer)
        }
        FlushPolicy::WhenIdle => None,
    }
}

/// Wait for the next tick of `timer`, or forever if there is no timer.
pub(crate) async fn next_tick(timer: &mut Option<Interval>) {
    match timer {
        Some(timer) => {
            timer.tick().await;
        }
        None => std::future::pending().await,
    }
}
