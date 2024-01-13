// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{PathSegment, ServerError};
use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, Histogram, HistogramVec, IntCounterVec, Registry,
};
use std::fmt::Write;

use crate::error::code;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub db_metrics: Arc<DBMetrics>,
    pub request_metrics: Arc<RequestMetrics>,
}

impl Metrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        let db_metrics = DBMetrics::new(&registry);
        let request_metrics = RequestMetrics::new(&registry);

        Self {
            db_metrics: Arc::new(db_metrics),
            request_metrics: Arc::new(request_metrics),
        }
    }

    /// Updates the DB related metrics (latency, error, success)
    pub(crate) fn observe_db_data(&self, db_latency: f64, succeeded: bool) {
        self.db_metrics
            .db_fetch_latency
            .with_label_values(&["latency", "db"])
            .observe(db_latency);
        if succeeded {
            self.db_metrics
                .db_num_fetches
                .with_label_values(&["success"])
                .inc();
        } else {
            self.db_metrics
                .db_num_fetches
                .with_label_values(&["error"])
                .inc();
        }
    }

    /// Use this function to increment the number of errors due to bad input
    pub(crate) fn inc_num_bad_input_errors(&self, label: &str) {
        self.request_metrics
            .num_bad_input_errors_by_path
            // TODO when measuring cost related errors, there's no path
            // not 100% how to label such an error?
            .with_label_values(&[label])
            .inc();
    }

    /// Use this function to increment the number of errors per path, and labels
    /// them as either internal server error, bad user input, or any other error.
    /// The path is the same as the path that the user sees.
    pub(crate) fn inc_errors(&self, errors: Vec<ServerError>) {
        for err in errors {
            if let Some(ext) = err.extensions {
                if let Some(code) = ext.get("code") {
                    if let async_graphql_value::Value::String(val) = code.clone().into_value() {
                        if val == code::INTERNAL_SERVER_ERROR {
                            self.request_metrics
                                .num_internal_errors_by_path
                                .with_label_values(&[&self.get_query_path(err.path)])
                                .inc();
                        } else if val == code::BAD_USER_INPUT {
                            self.request_metrics
                                .num_bad_input_errors_by_path
                                .with_label_values(&[&self.get_query_path(err.path)])
                                .inc();
                        } else {
                            self.request_metrics
                                .num_query_errors_by_path
                                .with_label_values(&[&self.get_query_path(err.path)])
                                .inc();
                        }
                    }
                }
            }
        }
    }

    /// Use this function to increment the number of request error.
    /// If you need to increase error rate due to bad input, use both
    /// this function and `inc_error_bad_input`
    pub(crate) fn inc_num_query_success(&self) {
        self.request_metrics
            .num_query_success
            .with_label_values(&["path"])
            .inc();
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
    pub db_num_fetches: IntCounterVec,
    pub db_fetch_latency: HistogramVec,
    // TODO make this work
    pub _db_query_cost: Histogram,
    // TODO determine if we want this metric, and implement it
    pub _db_fetch_batch_size: HistogramVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.0001, 0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

const BATCH_SIZE_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];

impl DBMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            db_num_fetches: register_int_counter_vec_with_registry!(
                "db_fetch",
                "The total number of DB requests grouped by success or error",
                &["type"],
                registry
            )
            .unwrap(),

            db_fetch_latency: register_histogram_vec_with_registry!(
                "db_fetch_latency",
                "Latency of DB requests",
                &["latency", "type"],
                LATENCY_SEC_BUCKETS.to_vec(),
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
                &["batch_size", "type"],
                BATCH_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
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
    pub query_payload_error: IntCounterVec,
    /// The time it takes to validate the query
    pub query_validation_latency: Histogram,
    /// The time it takes to validate the query
    pub query_validation_latency_by_path: HistogramVec,
    /// The time it takes for the GraphQL service to execute the request
    pub query_latency: Histogram,
    // TODO figure out how to get the path for a
    /// The time it takes for the GraphQL service to execute the request by path
    pub _query_latency_by_path: HistogramVec,
    /// Number of query errors by path, which are not bad input or internal
    pub num_query_errors_by_path: IntCounterVec,
    /// Number of query successes by path
    pub num_query_success: IntCounterVec,
    /// Bad input errors by path
    pub num_bad_input_errors_by_path: IntCounterVec,
    /// Internal errors by path
    pub num_internal_errors_by_path: IntCounterVec,
}

// TODO: finetune buckets as we learn more about the distribution of queries
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
    100., 200., 400., 800., 1200., 1600., 2400., 3200., 4800., 6400., 9600., 12800., 25600.,
    51200., 102400.,
];
const DB_QUERY_COST_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];

// const REQUEST_NUM_ERRORS_BUCKETS: &[f64] = &[
//     100., 200., 400., 800., 1200., 1600., 2400., 3200., 4800., 6400., 9600., 12800., 25600.,
//     51200., 102400.,
// ];

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
                registry,
            )
            .unwrap(),
            query_payload_size: register_histogram_with_registry!(
                "query_payload_size",
                "Size of the query payload string",
                QUERY_PAYLOAD_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_payload_error: register_int_counter_vec_with_registry!(
                "query_payload_error",
                "The total number of client input errors due to too large payload size",
                &["path"],
                registry,
            )
            .unwrap(),
            query_validation_latency_by_path: register_histogram_vec_with_registry!(
                "query_validation_latency_by_path",
                "The time to validate the query for each path",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
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
            _query_latency_by_path: register_histogram_vec_with_registry!(
                "query_latency_by_path",
                "The time needed to resolve and get the result for the request for this path",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            num_query_errors_by_path: register_int_counter_vec_with_registry!(
                "num_query_errors",
                "The number of queries that resulted in an error",
                &["path"],
                registry,
            )
            .unwrap(),
            num_query_success: register_int_counter_vec_with_registry!(
                "num_query_success",
                "The number of queries that were successful",
                &["path"],
                registry,
            )
            .unwrap(),
            num_bad_input_errors_by_path: register_int_counter_vec_with_registry!(
                "num_bad_input_errors",
                "The number of requests that resulted in an error due to bad input from the client",
                &["path"],
                registry,
            )
            .unwrap(),
            num_internal_errors_by_path: register_int_counter_vec_with_registry!(
                "num_internal_errors_by_path",
                "The number of requests that resulted in an error due to internal error",
                &["path"],
                registry,
            )
            .unwrap(),
        }
    }
}
