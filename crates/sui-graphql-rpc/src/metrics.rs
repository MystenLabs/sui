// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{PathSegment, ServerError};
use mysten_metrics::histogram::{Histogram, HistogramVec};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry, IntCounter,
    IntCounterVec, Registry,
};
use std::fmt::Write;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub db_metrics: Arc<DBMetrics>,
    pub request_metrics: Arc<RequestMetrics>,
}

impl Metrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        let db_metrics = DBMetrics::new(registry);
        let request_metrics = RequestMetrics::new(registry);

        Self {
            db_metrics: Arc::new(db_metrics),
            request_metrics: Arc::new(request_metrics),
        }
    }

    /// Updates the DB related metrics (latency, error, success)
    pub(crate) fn observe_db_data(&self, time: u64, succeeded: bool) {
        let label = if succeeded { "success" } else { "error" };
        self.db_metrics.db_fetches.with_label_values(&[label]).inc();
        self.db_metrics
            .db_fetch_latency
            .with_label_values(&[label])
            .report(time);
    }

    /// Record the total time needed for handling the query
    pub(crate) fn query_latency(&self, time: u64) {
        self.request_metrics.query_latency.observe(time);
    }

    /// Record the time needed for validation the query
    pub(crate) fn query_validation_latency(&self, time: u64) {
        self.request_metrics.query_validation_latency.observe(time);
    }

    /// Increment the total number of queries by one
    pub(crate) fn inc_num_queries(&self) {
        self.request_metrics.num_queries.inc();
    }

    /// Use this function to increment the number of errors per path and per error type.
    /// The error type is detected automatically from the passed errors.
    pub(crate) fn inc_errors(&self, errors: Vec<ServerError>) {
        for err in errors {
            if let Some(ext) = err.extensions {
                if let Some(code) = ext.get("code") {
                    self.request_metrics
                        .num_errors
                        .with_label_values(&[&self.get_query_path(err.path), &code.to_string()])
                        .inc();
                }
            }
        }
    }

    /// When an error occurs, GraphQL returns a vector of PathSegments,
    /// that we can use to construct a simplified path to the actual error.
    pub(crate) fn get_query_path(&self, query: Vec<PathSegment>) -> String {
        let mut path = String::new();
        for (idx, s) in query.iter().enumerate() {
            if idx > 0 {
                path.push('.');
            }
            match s {
                PathSegment::Index(idx) => {
                    let _ = write!(&mut path, "{}", idx);
                }
                PathSegment::Field(name) => {
                    let _ = write!(&mut path, "{}", name);
                }
            }
        }
        path
    }
}

#[derive(Clone)]
pub(crate) struct DBMetrics {
    /// The number of fetches grouped by result (success or error)
    pub db_fetches: IntCounterVec,
    /// The fetch latency grouped by result (success or error)
    pub db_fetch_latency: HistogramVec,
    // TODO make this work, blocked by pg.rs (unclear if to use log function or smth else)
    pub _db_query_cost: Histogram,
    // TODO determine if we want this metric, and implement it
    pub _db_fetch_batch_size: HistogramVec,
}

impl DBMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            db_fetches: register_int_counter_vec_with_registry!(
                "db_fetches",
                "The number of fetches grouped by result (success or error)",
                &["type"],
                registry
            )
            .unwrap(),
            db_fetch_latency: HistogramVec::new_in_registry(
                "db_fetch_latency",
                "The fetch latency grouped by result (success or error)",
                &["type"],
                registry,
            ),
            _db_query_cost: Histogram::new_in_registry(
                "db_query_cost",
                "Cost of a DB query",
                registry,
            ),
            _db_fetch_batch_size: HistogramVec::new_in_registry(
                "db_fetch_batch_size",
                "Number of ids fetched per batch",
                &["type"],
                registry,
            ),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestMetrics {
    /// The number of nodes for the input query that passed the query limits check
    pub input_nodes: Histogram,
    /// The number of nodes in the result
    pub output_nodes: Histogram,
    /// The query depth
    pub query_depth: Histogram,
    /// The size (in bytes) of the payload
    pub query_payload_size: Histogram,
    /// An error due to too high payload size
    pub query_payload_error: IntCounter,
    /// The time it takes to validate the query
    pub query_validation_latency: Histogram,
    /// The time it takes to validate the query
    pub _query_validation_latency_by_path: HistogramVec,
    /// The time it takes for the GraphQL service to execute the request
    pub query_latency: Histogram,
    // TODO figure out how to formalize a query path
    /// The time it takes for the GraphQL service to execute the request by path
    pub _query_latency_by_path: HistogramVec,
    /// Number of errors by path and type
    pub num_errors: IntCounterVec,
    /// Number of queries
    pub num_queries: IntCounter,
}

impl RequestMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            input_nodes: Histogram::new_in_registry(
                "input_nodes",
                "Number of input nodes in the query",
                registry,
            ),
            output_nodes: Histogram::new_in_registry(
                "output_nodes",
                "Number of output nodes in the response",
                registry,
            ),
            query_depth: Histogram::new_in_registry("query_depth", "Depth of the query", registry),
            query_payload_size: Histogram::new_in_registry(
                "query_payload_size",
                "Size of the query payload string",
                registry,
            ),
            query_payload_error: register_int_counter_with_registry!(
                "query_payload_error",
                "The total number of client input errors due to too large payload size",
                registry,
            )
            .unwrap(),
            _query_validation_latency_by_path: HistogramVec::new_in_registry(
                "query_validation_latency_by_path",
                "The time to validate the query for each path",
                &["path"],
                registry,
            ),
            query_validation_latency: Histogram::new_in_registry(
                "query_validation_latency",
                "The time to validate the query",
                registry,
            ),
            query_latency: Histogram::new_in_registry(
                "query_latency",
                "The time needed to resolve and get the result for the request",
                registry,
            ),
            _query_latency_by_path: HistogramVec::new_in_registry(
                "query_latency_by_path",
                "The time needed to resolve and get the result for the request for this path",
                &["path"],
                registry,
            ),
            num_errors: register_int_counter_vec_with_registry!(
                "num_errors",
                "Number of errors by path and error type",
                &["path", "type"],
                registry,
            )
            .unwrap(),
            num_queries: register_int_counter_with_registry!(
                "num_queries",
                "Total number of queries",
                registry
            )
            .unwrap(),
        }
    }
}
