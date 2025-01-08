// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct QueryMetrics {
    pub latency_ms: Vec<f64>,
    pub errors: usize,
    pub total_queries: usize,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub total_queries: usize,
    pub total_errors: usize,
    pub avg_latency_ms: f64,
    pub table_stats: Vec<TableStats>,
}

#[derive(Debug)]
pub struct TableStats {
    pub table_name: String,
    pub queries: usize,
    pub errors: usize,
    pub avg_latency_ms: f64,
}

#[derive(Clone)]
pub struct MetricsCollector {
    metrics: Arc<Mutex<HashMap<String, QueryMetrics>>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl MetricsCollector {
    pub fn record_query(&self, query_type: &str, latency: Duration, is_error: bool) {
        let mut metrics = self.metrics.lock().unwrap();
        let metrics = metrics.entry(query_type.to_string()).or_default();

        metrics.total_queries += 1;
        if is_error {
            metrics.errors += 1;
        } else {
            metrics.latency_ms.push(latency.as_secs_f64() * 1000.0);
        }
    }

    pub fn generate_report(&self) -> BenchmarkResult {
        let metrics = self.metrics.lock().unwrap();
        let mut total_queries = 0;
        let mut total_errors = 0;
        let mut total_latency = 0.0;
        let mut total_successful = 0;
        let mut table_stats = Vec::new();

        for (table_name, metrics) in metrics.iter() {
            let successful = metrics.total_queries - metrics.errors;
            let avg_latency = if successful > 0 {
                metrics.latency_ms.iter().sum::<f64>() / successful as f64
            } else {
                0.0
            };

            table_stats.push(TableStats {
                table_name: table_name.clone(),
                queries: metrics.total_queries,
                errors: metrics.errors,
                avg_latency_ms: avg_latency,
            });

            total_queries += metrics.total_queries;
            total_errors += metrics.errors;
            total_latency += metrics.latency_ms.iter().sum::<f64>();
            total_successful += successful;
        }
        table_stats.sort_by(|a, b| b.queries.cmp(&a.queries));

        BenchmarkResult {
            total_queries,
            total_errors,
            avg_latency_ms: if total_successful > 0 {
                total_latency / total_successful as f64
            } else {
                0.0
            },
            table_stats,
        }
    }
}
