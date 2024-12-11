// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use dashmap::DashMap;
use parking_lot::Mutex;
use prometheus::core::{AtomicI64, GenericGauge};
use simple_server_timing_header::Timer;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use once_cell::sync::OnceCell;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, Histogram, IntCounterVec, IntGaugeVec, Registry,
    TextEncoder,
};
use tap::TapFallible;
use tracing::{warn, Span};

pub use scopeguard;
use uuid::Uuid;

mod guards;
pub mod histogram;
pub mod metered_channel;
pub mod monitored_mpsc;
pub mod thread_stall_monitor;
pub use guards::*;

pub const TX_TYPE_SINGLE_WRITER_TX: &str = "single_writer";
pub const TX_TYPE_SHARED_OBJ_TX: &str = "shared_object";

pub const SUBSECOND_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.02, 0.03, 0.05, 0.075, 0.1, 0.2, 0.3, 0.5, 0.7, 1.,
];

pub const COARSE_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.2, 0.3, 0.5, 0.7, 1., 2., 3., 5., 10., 20., 30., 60.,
];

pub const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.6,
    0.7, 0.8, 0.9, 1., 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2., 2.5, 3., 3.5, 4., 4.5, 5.,
    6., 7., 8., 9., 10., 15., 20., 25., 30., 60., 90.,
];

pub const COUNT_BUCKETS: &[f64] = &[
    2., 5., 10., 20., 50., 100., 200., 500., 1000., 2000., 5000., 10000.,
];

pub const BYTES_BUCKETS: &[f64] = &[
    1024., 4096., 16384., 65536., 262144., 524288., 1048576., 2097152., 4194304., 8388608.,
    16777216., 33554432., 67108864.,
];

#[derive(Debug)]
pub struct Metrics {
    pub tasks: IntGaugeVec,
    pub futures: IntGaugeVec,
    pub channel_inflight: IntGaugeVec,
    pub channel_sent: IntGaugeVec,
    pub channel_received: IntGaugeVec,
    pub future_active_duration_ns: IntGaugeVec,
    pub scope_iterations: IntGaugeVec,
    pub scope_duration_ns: IntGaugeVec,
    pub scope_entrance: IntGaugeVec,
    pub thread_stall_duration_sec: Histogram,
    pub system_invariant_violations: IntCounterVec,
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
            channel_inflight: register_int_gauge_vec_with_registry!(
                "monitored_channel_inflight",
                "Inflight items in channels.",
                &["name"],
                registry,
            )
            .unwrap(),
            channel_sent: register_int_gauge_vec_with_registry!(
                "monitored_channel_sent",
                "Sent items in channels.",
                &["name"],
                registry,
            )
            .unwrap(),
            channel_received: register_int_gauge_vec_with_registry!(
                "monitored_channel_received",
                "Received items in channels.",
                &["name"],
                registry,
            )
            .unwrap(),
            future_active_duration_ns: register_int_gauge_vec_with_registry!(
                "monitored_future_active_duration_ns",
                "Total duration in nanosecs where the monitored future is active (consuming CPU time)",
                &["name"],
                registry,
            )
            .unwrap(),
            scope_entrance: register_int_gauge_vec_with_registry!(
                "monitored_scope_entrance",
                "Number of entrance in the scope.",
                &["name"],
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
            thread_stall_duration_sec: register_histogram_with_registry!(
                "thread_stall_duration_sec",
                "Duration of thread stalls in seconds.",
                registry,
            )
            .unwrap(),
            system_invariant_violations: register_int_counter_vec_with_registry!(
                "system_invariant_violations",
                "Number of system invariant violations",
                &["name"],
                registry,
            ).unwrap(),
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

tokio::task_local! {
    static SERVER_TIMING: Arc<Mutex<Timer>>;
}

/// Create a new task-local ServerTiming context and run the provided future within it.
/// Should be used at the top-most level of a request handler. Can be added to an axum router
/// as a layer by using mysten_service::server_timing_middleware.
pub async fn with_new_server_timing<T>(fut: impl Future<Output = T> + Send + 'static) -> T {
    let timer = Arc::new(Mutex::new(Timer::new()));

    let mut ret = None;
    SERVER_TIMING
        .scope(timer, async {
            ret = Some(fut.await);
        })
        .await;

    ret.unwrap()
}

/// Create a new task-local ServerTiming context and run the provided future within it.
/// Only intended for use by macros within this module.
pub async fn with_server_timing<T>(
    timer: Arc<Mutex<Timer>>,
    fut: impl Future<Output = T> + Send + 'static,
) -> T {
    let mut ret = None;
    SERVER_TIMING
        .scope(timer, async {
            ret = Some(fut.await);
        })
        .await;

    ret.unwrap()
}

/// Get the currently active ServerTiming context. Only intended for use by macros within this module.
pub fn get_server_timing() -> Option<Arc<Mutex<Timer>>> {
    SERVER_TIMING.try_with(|timer| timer.clone()).ok()
}

/// Add a new entry to the ServerTiming header.
/// If the caller is not currently in a ServerTiming context (created with `with_new_server_timing`),
/// an error is logged.
pub fn add_server_timing(name: &str) {
    let res = SERVER_TIMING.try_with(|timer| {
        timer.lock().add(name);
    });

    if res.is_err() {
        tracing::error!("Server timing context not found");
    }
}

#[macro_export]
macro_rules! monitored_future {
    ($fut: expr) => {{
        monitored_future!(futures, $fut, "", INFO, false)
    }};

    ($metric: ident, $fut: expr, $name: expr, $logging_level: ident, $logging_enabled: expr) => {{
        let location: &str = if $name.is_empty() {
            concat!(file!(), ':', line!())
        } else {
            concat!(file!(), ':', $name)
        };

        async move {
            let metrics = $crate::get_metrics();

            let _metrics_guard = if let Some(m) = metrics {
                m.$metric.with_label_values(&[location]).inc();
                Some($crate::scopeguard::guard(m, |_| {
                    m.$metric.with_label_values(&[location]).dec();
                }))
            } else {
                None
            };
            let _logging_guard = if $logging_enabled {
                Some($crate::scopeguard::guard((), |_| {
                    tracing::event!(
                        tracing::Level::$logging_level,
                        "Future {} completed",
                        location
                    );
                }))
            } else {
                None
            };

            if $logging_enabled {
                tracing::event!(
                    tracing::Level::$logging_level,
                    "Spawning future {}",
                    location
                );
            }

            $fut.await
        }
    }};
}

#[macro_export]
macro_rules! forward_server_timing_and_spawn {
    ($fut: expr) => {
        if let Some(timing) = $crate::get_server_timing() {
            tokio::task::spawn(async move { $crate::with_server_timing(timing, $fut).await })
        } else {
            tokio::task::spawn($fut)
        }
    };
}

#[macro_export]
macro_rules! spawn_monitored_task {
    ($fut: expr) => {
        $crate::forward_server_timing_and_spawn!($crate::monitored_future!(
            tasks, $fut, "", INFO, false
        ))
    };
}

#[macro_export]
macro_rules! spawn_logged_monitored_task {
    ($fut: expr) => {
        $crate::forward_server_timing_and_spawn!($crate::monitored_future!(
            tasks, $fut, "", INFO, true
        ))
    };

    ($fut: expr, $name: expr) => {
        $crate::forward_server_timing_and_spawn!($crate::monitored_future!(
            tasks, $fut, $name, INFO, true
        ))
    };

    ($fut: expr, $name: expr, $logging_level: ident) => {
        $crate::forward_server_timing_and_spawn!($crate::monitored_future!(
            tasks,
            $fut,
            $name,
            $logging_level,
            true
        ))
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
        self.metrics
            .scope_entrance
            .with_label_values(&[self.name])
            .dec();
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
        m.scope_entrance.with_label_values(&[name]).inc();
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
            active_duration_metric: get_metrics()
                .map(|m| m.future_active_duration_ns.with_label_values(&[name])),
            _scope: monitored_scope(name),
        }
    }
}

pub struct MonitoredScopeFuture<F: Sized> {
    f: Pin<Box<F>>,
    active_duration_metric: Option<GenericGauge<AtomicI64>>,
    _scope: Option<MonitoredScopeGuard>,
}

impl<F: Future> Future for MonitoredScopeFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let active_timer = Instant::now();
        let ret = self.f.as_mut().poll(cx);
        if let Some(m) = &self.active_duration_metric {
            m.add(active_timer.elapsed().as_nanos() as i64);
        }
        ret
    }
}

pub struct CancelMonitor<F: Sized> {
    finished: bool,
    inner: Pin<Box<F>>,
}

impl<F> CancelMonitor<F>
where
    F: Future,
{
    pub fn new(inner: F) -> Self {
        Self {
            finished: false,
            inner: Box::pin(inner),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl<F> Future for CancelMonitor<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.inner.as_mut().poll(cx) {
            Poll::Ready(output) => {
                self.finished = true;
                Poll::Ready(output)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<F: Sized> Drop for CancelMonitor<F> {
    fn drop(&mut self) {
        if !self.finished {
            Span::current().record("cancelled", true);
        }
    }
}

/// MonitorCancellation records a cancelled = true span attribute if the future it
/// is decorating is dropped before completion. The cancelled attribute must be added
/// at span creation, as you cannot add new attributes after the span is created.
pub trait MonitorCancellation {
    fn monitor_cancellation(self) -> CancelMonitor<Self>
    where
        Self: Sized + Future;
}

impl<T> MonitorCancellation for T
where
    T: Future,
{
    fn monitor_cancellation(self) -> CancelMonitor<Self> {
        CancelMonitor::new(self)
    }
}

pub type RegistryID = Uuid;

/// A service to manage the prometheus registries. This service allow us to create
/// a new Registry on demand and keep it accessible for processing/polling.
/// The service can be freely cloned/shared across threads.
#[derive(Clone)]
pub struct RegistryService {
    // Holds a Registry that is supposed to be used
    default_registry: Registry,
    registries_by_id: Arc<DashMap<Uuid, Registry>>,
}

impl RegistryService {
    // Creates a new registry service and also adds the main/default registry that is supposed to
    // be preserved and never get removed
    pub fn new(default_registry: Registry) -> Self {
        Self {
            default_registry,
            registries_by_id: Arc::new(DashMap::new()),
        }
    }

    // Returns the default registry for the service that someone can use
    // if they don't want to create a new one.
    pub fn default_registry(&self) -> Registry {
        self.default_registry.clone()
    }

    // Adds a new registry to the service. The corresponding RegistryID is returned so can later be
    // used for removing the Registry. Method panics if we try to insert a registry with the same id.
    // As this can be quite serious for the operation of the node we don't want to accidentally
    // swap an existing registry - we expected a removal to happen explicitly.
    pub fn add(&self, registry: Registry) -> RegistryID {
        let registry_id = Uuid::new_v4();
        if self
            .registries_by_id
            .insert(registry_id, registry)
            .is_some()
        {
            panic!("Other Registry already detected for the same id {registry_id}");
        }

        registry_id
    }

    // Removes the registry from the service. If Registry existed then this method returns true,
    // otherwise false is returned instead.
    pub fn remove(&self, registry_id: RegistryID) -> bool {
        self.registries_by_id.remove(&registry_id).is_some()
    }

    // Returns all the registries of the service
    pub fn get_all(&self) -> Vec<Registry> {
        let mut registries: Vec<Registry> = self
            .registries_by_id
            .iter()
            .map(|r| r.value().clone())
            .collect();
        registries.push(self.default_registry.clone());

        registries
    }

    // Returns all the metric families from the registries that a service holds.
    pub fn gather_all(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.get_all().iter().flat_map(|r| r.gather()).collect()
    }
}

/// Create a metric that measures the uptime from when this metric was constructed.
/// The metric is labeled with:
/// - 'process': the process type, differentiating between validator and fullnode
/// - 'version': binary version, generally be of the format: 'semver-gitrevision'
/// - 'chain_identifier': the identifier of the network which this process is part of
pub fn uptime_metric(
    process: &str,
    version: &'static str,
    chain_identifier: &str,
) -> Box<dyn prometheus::core::Collector> {
    let opts = prometheus::opts!("uptime", "uptime of the node service in seconds")
        .variable_label("process")
        .variable_label("version")
        .variable_label("chain_identifier");

    let start_time = std::time::Instant::now();
    let uptime = move || start_time.elapsed().as_secs();
    let metric = prometheus_closure_metric::ClosureMetric::new(
        opts,
        prometheus_closure_metric::ValueType::Counter,
        uptime,
        &[process, version, chain_identifier],
    )
    .unwrap();

    Box::new(metric)
}

/// Similar to `uptime_metric`, but for the bridge node with different labels.
/// Create a metric that measures the uptime from when this metric was constructed.
/// The metric is labeled with:
/// - 'process': the process type. We keep this label to be able to distinguish between different binaries.
/// - 'version': binary version, generally be of the format: 'semver-gitrevision'
/// - 'sui_chain_identifier': the identifier of sui network which this process is part of
/// - 'eth_chain_identifier': the identifier of eth network which this process is part of
/// - 'client_enabled': whether the bridge node is running as a client
pub fn bridge_uptime_metric(
    process: &str,
    version: &'static str,
    sui_chain_identifier: &str,
    eth_chain_identifier: &str,
    client_enabled: bool,
) -> Box<dyn prometheus::core::Collector> {
    let opts = prometheus::opts!("uptime", "uptime of the node service in seconds")
        .variable_label("process")
        .variable_label("version")
        .variable_label("sui_chain_identifier")
        .variable_label("eth_chain_identifier")
        .variable_label("client_enabled");

    let start_time = std::time::Instant::now();
    let uptime = move || start_time.elapsed().as_secs();
    let metric = prometheus_closure_metric::ClosureMetric::new(
        opts,
        prometheus_closure_metric::ValueType::Counter,
        uptime,
        &[
            process,
            version,
            sui_chain_identifier,
            eth_chain_identifier,
            if client_enabled { "true" } else { "false" },
        ],
    )
    .unwrap();

    Box::new(metric)
}

pub const METRICS_ROUTE: &str = "/metrics";

// Creates a new http server that has as a sole purpose to expose
// and endpoint that prometheus agent can use to poll for the metrics.
// A RegistryService is returned that can be used to get access in prometheus Registries.
pub fn start_prometheus_server(addr: SocketAddr) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    if cfg!(msim) {
        // prometheus uses difficult-to-support features such as TcpSocket::from_raw_fd(), so we
        // can't yet run it in the simulator.
        warn!("not starting prometheus server in simulator");
        return registry_service;
    }

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    registry_service
}

pub async fn metrics(
    Extension(registry_service): Extension<RegistryService>,
) -> (StatusCode, String) {
    let metrics_families = registry_service.gather_all();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::RegistryService;
    use prometheus::IntCounter;
    use prometheus::Registry;

    #[test]
    fn registry_service() {
        // GIVEN
        let default_registry = Registry::new_custom(Some("default".to_string()), None).unwrap();

        let registry_service = RegistryService::new(default_registry.clone());
        let default_counter = IntCounter::new("counter", "counter_desc").unwrap();
        default_counter.inc();
        default_registry
            .register(Box::new(default_counter))
            .unwrap();

        // AND add a metric to the default registry

        // AND a registry with one metric
        let registry_1 = Registry::new_custom(Some("narwhal".to_string()), None).unwrap();
        registry_1
            .register(Box::new(
                IntCounter::new("counter_1", "counter_1_desc").unwrap(),
            ))
            .unwrap();

        // WHEN
        let registry_1_id = registry_service.add(registry_1);

        // THEN
        let mut metrics = registry_service.gather_all();
        metrics.sort_by(|m1, m2| Ord::cmp(m1.get_name(), m2.get_name()));

        assert_eq!(metrics.len(), 2);

        let metric_default = metrics.remove(0);
        assert_eq!(metric_default.get_name(), "default_counter");
        assert_eq!(metric_default.get_help(), "counter_desc");

        let metric_1 = metrics.remove(0);
        assert_eq!(metric_1.get_name(), "narwhal_counter_1");
        assert_eq!(metric_1.get_help(), "counter_1_desc");

        // AND add a second registry with a metric
        let registry_2 = Registry::new_custom(Some("sui".to_string()), None).unwrap();
        registry_2
            .register(Box::new(
                IntCounter::new("counter_2", "counter_2_desc").unwrap(),
            ))
            .unwrap();
        let _registry_2_id = registry_service.add(registry_2);

        // THEN all the metrics should be returned
        let mut metrics = registry_service.gather_all();
        metrics.sort_by(|m1, m2| Ord::cmp(m1.get_name(), m2.get_name()));

        assert_eq!(metrics.len(), 3);

        let metric_default = metrics.remove(0);
        assert_eq!(metric_default.get_name(), "default_counter");
        assert_eq!(metric_default.get_help(), "counter_desc");

        let metric_1 = metrics.remove(0);
        assert_eq!(metric_1.get_name(), "narwhal_counter_1");
        assert_eq!(metric_1.get_help(), "counter_1_desc");

        let metric_2 = metrics.remove(0);
        assert_eq!(metric_2.get_name(), "sui_counter_2");
        assert_eq!(metric_2.get_help(), "counter_2_desc");

        // AND remove first registry
        assert!(registry_service.remove(registry_1_id));

        // THEN metrics should now not contain metric of registry_1
        let mut metrics = registry_service.gather_all();
        metrics.sort_by(|m1, m2| Ord::cmp(m1.get_name(), m2.get_name()));

        assert_eq!(metrics.len(), 2);

        let metric_default = metrics.remove(0);
        assert_eq!(metric_default.get_name(), "default_counter");
        assert_eq!(metric_default.get_help(), "counter_desc");

        let metric_1 = metrics.remove(0);
        assert_eq!(metric_1.get_name(), "sui_counter_2");
        assert_eq!(metric_1.get_help(), "counter_2_desc");
    }
}
