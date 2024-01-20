// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{PathSegment, ServerError};
use mysten_metrics::histogram::{Histogram, HistogramVec};
use prometheus::{
    register_gauge_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Gauge, IntCounter, IntCounterVec, Registry,
};

use crate::error::code;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub db_metrics: Arc<DBMetrics>,
    pub request_metrics: Arc<RequestMetrics>,
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

#[derive(Clone)]
pub(crate) struct RequestMetrics {
    /// The number of nodes for the input query that passed the query limits check
    pub input_nodes: Histogram,
    /// The number of nodes in the result
    pub output_nodes: Histogram,
    /// The query depth
    pub query_depth: Histogram,
    /// The size (in bytes) of the payload that is higher than the maximum
    pub query_payload_too_large_size: Histogram,
    /// The size (in bytes) of the payload
    pub query_payload_size: Histogram,
    /// The time it takes to validate the query
    pub query_validation_latency: Histogram,
    /// The time it takes for the GraphQL service to execute the request
    pub query_latency: Histogram,
    /// Number of errors by path and type.
    pub num_errors: IntCounterVec,
    /// Number of queries
    pub num_queries: IntCounter,
    /// Number of queries by top level path
    pub num_queries_top_level: IntCounterVec,
    /// Total inflight requests
    pub inflight_requests: Gauge,
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

    /// The total time needed for handling the query
    pub(crate) fn query_latency(&self, time: u64) {
        self.request_metrics.query_latency.observe(time);
    }

    /// The time needed for validating the query
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
                if let Some(async_graphql_value::ConstValue::String(val)) = ext.get("code") {
                    self.request_metrics
                        .num_errors
                        .with_label_values(&[query_label_for_error(&err.path).as_str(), val])
                        .inc();
                }
            } else {
                self.request_metrics
                    .num_errors
                    .with_label_values(&[query_label_for_error(&err.path).as_str(), code::UNKNOWN])
                    .inc();
            }
        }
    }
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
            query_payload_too_large_size: Histogram::new_in_registry(
                "query_payload_too_large_size",
                "Query payload size (bytes), that was rejected due to being larger than maximum",
                registry,
            ),
            query_payload_size: Histogram::new_in_registry(
                "query_payload_size",
                "Size of the query payload string",
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
            num_queries_top_level: register_int_counter_vec_with_registry!(
                "num_queries_top_level",
                "Number of queries for each top level node",
                &["path"],
                registry
            )
            .unwrap(),
            inflight_requests: register_gauge_with_registry!(
                "inflight_requests",
                "Number of queries that are being resolved at a moment in time",
                registry
            )
            .unwrap(),
        }
    }
}

/// When an error occurs, GraphQL returns a vector of PathSegments,
/// that we can use to retrieve the last node which contains the error.
pub(crate) fn query_label_for_error(query: &[PathSegment]) -> String {
    let fields: Vec<_> = query
        .iter()
        .filter_map(|s| {
            if let PathSegment::Field(name) = s {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    match &fields[..] {
        [] => "".to_string(),
        [seg] => seg.to_string(),
        [fst, .., lst] => format!("{fst}..{lst}"),
    }
}
