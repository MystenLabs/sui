// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Defines a structure to hold and manage metrics for a watchdog service.
/// This structure is thread-safe, allowing concurrent access and modification of metrics.
#[derive(Debug)]
pub struct WatchdogMetrics {
    // The Prometheus registry to which metrics are registered.
    registry: Registry,
    // A HashMap to store IntGauge metrics, keyed by their names.
    // Wrapped in a Mutex to ensure thread-safe access.
    metrics: Mutex<HashMap<String, IntGauge>>,
}

impl WatchdogMetrics {
    /// Constructs a new WatchdogMetrics instance with the given Prometheus registry
    pub fn new(registry: &Registry) -> Self {
        Self {
            registry: registry.clone(),
            metrics: Mutex::new(HashMap::new()),
        }
    }

    /// Retrieves or creates an metric for the specified metric name.
    /// The metric name is suffixed with "_exact" to denote its type.
    pub async fn get(&self, metric_name: &str) -> anyhow::Result<IntGauge> {
        let mut metrics = self.metrics.lock().await;
        // If the metric doesn't exist, register it and insert into the map.
        let metric = metrics.entry(metric_name.to_string()).or_insert_with(|| {
            register_int_gauge_with_registry!(metric_name, metric_name, &self.registry).unwrap()
        });
        Ok(metric.clone())
    }

    /// Retrieves or creates an "exact" metric for the specified metric name.
    /// The metric name is suffixed with "_exact" to denote its type.
    pub async fn get_exact(&self, metric_name: &str) -> anyhow::Result<IntGauge> {
        let mut metrics = self.metrics.lock().await;
        let metric_name = format!("{}_exact", metric_name);
        // If the metric doesn't exist, register it and insert into the map.
        let metric = metrics.entry(metric_name.clone()).or_insert_with(|| {
            register_int_gauge_with_registry!(&metric_name, &metric_name, &self.registry).unwrap()
        });
        Ok(metric.clone())
    }

    /// Similar to get_exact, but for "lower" bound metrics.
    pub async fn get_lower(&self, metric_name: &str) -> anyhow::Result<IntGauge> {
        let mut metrics = self.metrics.lock().await;
        let metric_name = format!("{}_lower", metric_name);
        let metric = metrics.entry(metric_name.clone()).or_insert_with(|| {
            register_int_gauge_with_registry!(&metric_name, &metric_name, &self.registry).unwrap()
        });
        Ok(metric.clone())
    }

    /// Similar to get_exact, but for "upper" bound metrics.
    pub async fn get_upper(&self, metric_name: &str) -> anyhow::Result<IntGauge> {
        let mut metrics = self.metrics.lock().await;
        let metric_name = format!("{}_upper", metric_name);
        let metric = metrics.entry(metric_name.clone()).or_insert_with(|| {
            register_int_gauge_with_registry!(&metric_name, &metric_name, &self.registry).unwrap()
        });
        Ok(metric.clone())
    }
}
