// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use once_cell::sync::OnceCell;
use prometheus::{register_int_gauge_vec_with_registry, IntGaugeVec, Registry};
use tap::TapFallible;
use tracing::warn;

pub use scopeguard;
use uuid::Uuid;

mod guards;
pub mod histogram;
pub use guards::*;

pub const TX_TYPE_SINGLE_WRITER_TX: &str = "single_writer";
pub const TX_TYPE_SHARED_OBJ_TX: &str = "shared_object";

#[derive(Debug)]
pub struct Metrics {
    pub tasks: IntGaugeVec,
    pub futures: IntGaugeVec,
    pub scope_iterations: IntGaugeVec,
    pub scope_duration_ns: IntGaugeVec,
    pub scope_entrance: IntGaugeVec,
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
        monitored_future!(futures, $fut, "", INFO, false)
    }};

    ($metric: ident, $fut: expr, $name: expr, $logging_level: ident, $logging_enabled: expr) => {{
        let location: &str = if $name.is_empty() {
            concat!(file!(), ':', line!())
        } else {
            concat!(file!(), ':', $name)
        };

        async move {
            let metrics = mysten_metrics::get_metrics();

            let _metrics_guard = if let Some(m) = metrics {
                m.$metric.with_label_values(&[location]).inc();
                Some(mysten_metrics::scopeguard::guard(m, |metrics| {
                    m.$metric.with_label_values(&[location]).dec();
                }))
            } else {
                None
            };
            let _logging_guard = if $logging_enabled {
                Some(mysten_metrics::scopeguard::guard((), |_| {
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
macro_rules! spawn_monitored_task {
    ($fut: expr) => {
        tokio::task::spawn(mysten_metrics::monitored_future!(
            tasks, $fut, "", INFO, false
        ))
    };
}

#[macro_export]
macro_rules! spawn_logged_monitored_task {
    ($fut: expr) => {
        tokio::task::spawn(mysten_metrics::monitored_future!(
            tasks, $fut, "", INFO, true
        ))
    };

    ($fut: expr, $name: expr) => {
        tokio::task::spawn(mysten_metrics::monitored_future!(
            tasks, $fut, $name, INFO, true
        ))
    };

    ($fut: expr, $name: expr, $logging_level: ident) => {
        tokio::task::spawn(mysten_metrics::monitored_future!(
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
/// The metric is labeled with the provided 'version' label (this should generally be of the
/// format: 'semver-gitrevision').
pub fn uptime_metric(version: &'static str) -> Box<dyn prometheus::core::Collector> {
    let opts = prometheus::opts!("uptime", "uptime of the node service in seconds")
        .variable_label("version");

    let start_time = std::time::Instant::now();
    let uptime = move || start_time.elapsed().as_secs();
    let metric = prometheus_closure_metric::ClosureMetric::new(
        opts,
        prometheus_closure_metric::ValueType::Counter,
        uptime,
        &[version],
    )
    .unwrap();

    Box::new(metric)
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
