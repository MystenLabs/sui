// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module defines data structures and functions for collecting
/// and summarizing performance metrics from benchmark queries. It
/// supports tracking overall and per-table query latencies, error counts, total queries,
use dashmap::DashMap;
use std::sync::Arc;
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

#[derive(Clone, Default)]
pub struct MetricsCollector {
    metrics: Arc<DashMap<String, QueryMetrics>>,
}

impl MetricsCollector {
    pub fn record_query(&self, query_type: &str, latency: Duration, is_error: bool) {
        let mut entry = self.metrics.entry(query_type.to_string()).or_default();

        entry.total_queries += 1;
        if is_error {
            entry.errors += 1;
        } else {
            entry.latency_ms.push(latency.as_secs_f64() * 1000.0);
        }
    }

    pub fn generate_report(&self) -> BenchmarkResult {
        let mut total_queries = 0;
        let mut total_errors = 0;
        let mut total_latency = 0.0;
        let mut total_successful = 0;
        let mut table_stats = Vec::new();

        for entry in self.metrics.iter() {
            let table_name = entry.key().clone();
            let metrics = entry.value();
            let successful = metrics.total_queries - metrics.errors;
            let avg_latency = if successful > 0 {
                metrics.latency_ms.iter().sum::<f64>() / successful as f64
            } else {
                0.0
            };

            table_stats.push(TableStats {
                table_name,
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
