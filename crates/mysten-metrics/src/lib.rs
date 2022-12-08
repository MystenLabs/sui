// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use once_cell::sync::OnceCell;
use prometheus::{register_int_gauge_vec_with_registry, IntGaugeVec, Registry};
use tap::TapFallible;
use tracing::warn;

pub use scopeguard;

#[derive(Debug)]
pub struct Metrics {
    pub tasks: IntGaugeVec,
    pub futures: IntGaugeVec,
    pub scope_iterations: IntGaugeVec,
    pub scope_duration_ns: IntGaugeVec,
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        Self {
            tasks: register_int_gauge_vec_with_registry!(
                "monitored_tasks",
                "Number of running tasks per callsite.",
                &["callsite"],
                registry,
            )
            .unwrap(),
            futures: register_int_gauge_vec_with_registry!(
                "monitored_futures",
                "Number of pending futures per callsite.",
                &["callsite"],
                registry,
            )
            .unwrap(),
            scope_iterations: register_int_gauge_vec_with_registry!(
                "monitored_scope_iterations",
                "Total number of times where the monitored scope runs",
                &["name"],
                registry,
            )
            .unwrap(),
            scope_duration_ns: register_int_gauge_vec_with_registry!(
                "monitored_scope_duration_ns",
                "Total duration in nanosecs where the monitored scope is running",
                &["name"],
                registry,
            )
            .unwrap(),
        }
    }
}

static METRICS: OnceCell<Metrics> = OnceCell::new();

pub fn init_metrics(registry: &Registry) {
    let _ = METRICS
        .set(Metrics::new(registry))
        // this happens many times during tests
        .tap_err(|_| warn!("init_metrics registry overwritten"));
}

pub fn get_metrics() -> Option<&'static Metrics> {
    METRICS.get()
}

#[macro_export]
macro_rules! monitored_future {
    ($fut: expr) => {{
        monitored_future!(futures, $fut)
    }};

    ($metric: ident, $fut: expr) => {{
        const LOCATION: &str = concat!(file!(), ':', line!());

        async move {
            let metrics = mysten_metrics::get_metrics();

            let _guard = if let Some(m) = metrics {
                m.$metric.with_label_values(&[LOCATION]).inc();
                Some(mysten_metrics::scopeguard::guard(m, |metrics| {
                    m.$metric.with_label_values(&[LOCATION]).dec();
                }))
            } else {
                None
            };

            $fut.await
        }
    }};
}

#[macro_export]
macro_rules! spawn_monitored_task {
    ($fut: expr) => {
        tokio::task::spawn(mysten_metrics::monitored_future!(tasks, $fut))
    };
}

pub struct MonitoredScopeGuard {
    metrics: &'static Metrics,
    name: &'static str,
    timer: Instant,
}

impl Drop for MonitoredScopeGuard {
    fn drop(&mut self) {
        self.metrics
            .scope_duration_ns
            .with_label_values(&[self.name])
            .add(self.timer.elapsed().as_nanos() as i64);
    }
}

/// This function creates a named scoped object, that keeps track of
/// - the total iterations where the scope is called in the `monitored_scope_iterations` metric.
/// - and the total duration of the scope in the `monitored_scope_duration_ns` metric.
///
/// The monitored scope should be single threaded, e.g. the scoped object encompass the lifetime of
/// a select loop or guarded by mutex.
/// Then the rate of `monitored_scope_duration_ns`, converted to the unit of sec / sec, would be
/// how full the single threaded scope is running.
pub fn monitored_scope(name: &'static str) -> Option<MonitoredScopeGuard> {
    let metrics = get_metrics();
    if let Some(m) = metrics {
        m.scope_iterations.with_label_values(&[name]).inc();
        Some(MonitoredScopeGuard {
            metrics: m,
            name,
            timer: Instant::now(),
        })
    } else {
        None
    }
}

pub trait MonitoredFutureExt: Future + Sized {
    fn in_monitored_scope(self, name: &'static str) -> MonitoredScopeFuture<Self>;
}

impl<F: Future> MonitoredFutureExt for F {
    fn in_monitored_scope(self, name: &'static str) -> MonitoredScopeFuture<Self> {
        MonitoredScopeFuture {
            f: Box::pin(self),
            _scope: monitored_scope(name),
        }
    }
}

pub struct MonitoredScopeFuture<F: Sized> {
    f: Pin<Box<F>>,
    _scope: Option<MonitoredScopeGuard>,
}

impl<F: Future> Future for MonitoredScopeFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.f.as_mut().poll(cx)
    }
}
