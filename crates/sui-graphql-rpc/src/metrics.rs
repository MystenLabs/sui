// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use async_graphql::{PathSegment, ServerError};
use prometheus::{
    register_gauge_with_registry, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Gauge, Histogram, HistogramVec, IntCounter, IntCounterVec,
    Registry,
};

use crate::error::code;

// TODO: finetune buckets as we learn more about the distribution of queries
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];
const DB_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 3.0,
    5.0, 10.0, 20.0, 40.0, 60.0, 80.0, 100.0, 200.0,
];
const INPUT_NODES_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];
const OUTPUT_NODES_BUCKETS: &[f64] = &[
    100., 200., 400., 800., 1200., 1600., 2400., 3200., 4800., 6400., 9600., 12800., 25600.,
    51200., 102400.,
];
const QUERY_DEPTH_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];
const QUERY_PAYLOAD_SIZE_BUCKETS: &[f64] = &[
    10., 20., 50., 100., 200., 400., 800., 1200., 1600., 2400., 3200., 4800., 6400., 9600., 12800.,
    25600., 51200., 102400.,
];
const DB_QUERY_COST_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];

#[derive(Clone)]
pub(crate) struct Metrics {
    pub db_metrics: Arc<DBMetrics>,
    pub request_metrics: Arc<RequestMetrics>,
    pub app_metrics: Arc<AppMetrics>,
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

#[derive(Clone)]
pub(crate) struct AppMetrics {
    /// The time it takes for the non-mainnet graphql service to resolve the mvr object from
    /// mainnet.
    pub external_mvr_resolution_latency: Histogram,
}

impl Metrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        let db_metrics = DBMetrics::new(registry);
        let request_metrics = RequestMetrics::new(registry);
        let app_metrics = AppMetrics::new(registry);

        Self {
            db_metrics: Arc::new(db_metrics),
            request_metrics: Arc::new(request_metrics),
            app_metrics: Arc::new(app_metrics),
        }
    }

    /// Updates the DB related metrics (latency, error, success)
    pub(crate) fn observe_db_data(&self, time: Duration, succeeded: bool) {
        let label = if succeeded { "success" } else { "error" };
        self.db_metrics.db_fetches.with_label_values(&[label]).inc();
        self.db_metrics
            .db_fetch_latency
            .with_label_values(&[label])
            .observe(time.as_secs_f64());
    }

    /// The total time needed for handling the query
    pub(crate) fn query_latency(&self, time: Duration) {
        self.request_metrics
            .query_latency
            .observe(time.as_secs_f64());
    }

    /// The time needed for validating the query
    pub(crate) fn query_validation_latency(&self, time: Duration) {
        self.request_metrics
            .query_validation_latency
            .observe(time.as_secs_f64());
    }

    /// Increment the total number of queries by one
    pub(crate) fn inc_num_queries(&self) {
        self.request_metrics.num_queries.inc();
    }

    /// Use this function to increment the number of errors per path and per error type.
    /// The error type is detected automatically from the passed errors.
    pub(crate) fn inc_errors(&self, errors: &[ServerError]) {
        for err in errors {
            if let Some(ext) = &err.extensions {
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
            db_fetch_latency: register_histogram_vec_with_registry!(
                "db_fetch_latency",
                "The fetch latency grouped by result (success or error)",
                &["type"],
                DB_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            _db_query_cost: register_histogram_with_registry!(
                "db_query_cost",
                "Cost of a DB query",
                DB_QUERY_COST_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            _db_fetch_batch_size: register_histogram_vec_with_registry!(
                "db_fetch_batch_size",
                "Number of ids fetched per batch",
                &["type"],
                registry,
            )
            .unwrap(),
        }
    }
}

impl RequestMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            input_nodes: register_histogram_with_registry!(
                "input_nodes",
                "Number of input nodes in the query",
                INPUT_NODES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            output_nodes: register_histogram_with_registry!(
                "output_nodes",
                "Number of output nodes in the response",
                OUTPUT_NODES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_depth: register_histogram_with_registry!(
                "query_depth",
                "Depth of the query",
                QUERY_DEPTH_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            query_payload_size: register_histogram_with_registry!(
                "query_payload_size",
                "Size of the query payload string",
                QUERY_PAYLOAD_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_validation_latency: register_histogram_with_registry!(
                "query_validation_latency",
                "The time to validate the query",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_latency: register_histogram_with_registry!(
                "query_latency",
                "The time needed to resolve and get the result for the request",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
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

impl AppMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            external_mvr_resolution_latency: register_histogram_with_registry!(
                "external_mvr_resolution_latency",
                "The time it takes for the non-mainnet graphql service to resolve the mvr object from mainnet",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
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
