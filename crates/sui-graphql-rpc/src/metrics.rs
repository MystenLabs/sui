// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_counter_vec_with_registry, register_counter_with_registry,
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec, register_int_counter_vec_with_registry, Histogram, HistogramVec,
    IntCounterVec, Registry,
};

use crate::error::Error;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub db_metrics: Arc<DBMetrics>,
    pub request_metrics: Arc<RequestMetrics>,
    pub query_metrics: Arc<QueryMetrics>,
}

impl Metrics {
    pub(crate) fn new(
        db_metrics: DBMetrics,
        request_metrics: RequestMetrics,
        query_metrics: QueryMetrics,
    ) -> Self {
        Self {
            db_metrics: Arc::new(db_metrics),
            request_metrics: Arc::new(request_metrics),
            query_metrics: Arc::new(query_metrics),
        }
    }

    pub(crate) fn default_none() -> Option<Self> {
        None
    }

    /// Updates the DB related metrics (latency, error, success)
    pub(crate) fn observe_db_data(&self, db_latency: u64, succeeded: bool) {
        self.db_metrics
            .db_fetch_latency
            .with_label_values(&["db", "latency"])
            .observe(db_latency as f64);
        if succeeded {
            self.db_metrics
                .db_fetch_success_rate
                .with_label_values(&["db", "success_rate"])
                .inc();
        } else {
            self.db_metrics
                .db_fetch_error_rate
                .with_label_values(&["db", "error_rate"])
                .inc();
        }
    }
}

#[derive(Clone)]
pub(crate) struct DBMetrics {
    pub db_fetch_success_rate: IntCounterVec,
    pub db_fetch_error_rate: IntCounterVec,
    pub db_fetch_latency: HistogramVec,
    pub db_fetch_batch_size: HistogramVec,
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
            db_fetch_error_rate: register_int_counter_vec_with_registry!(
                "db_fetch_error_rate",
                "The total number of DB requests that returned an error",
                &["db", "error_rate"],
                registry
            )
            .unwrap(),

            db_fetch_success_rate: register_int_counter_vec_with_registry!(
                "db_fetch_success_rate",
                "The total number of DB requests that were successful",
                &["db", "success_rate"],
                registry
            )
            .unwrap(),

            db_fetch_latency: register_histogram_vec_with_registry!(
                "db_fetch_latency",
                "Latency of DB requests",
                &["db", "latency"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            db_fetch_batch_size: register_histogram_vec_with_registry!(
                "db_fetch_batch_size",
                "Number of ids fetched per batch",
                &["db", "batch_size"],
                BATCH_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestMetrics {
    pub input_nodes: Histogram,
    pub output_nodes: Histogram,
    pub query_depth: Histogram,
    pub query_payload_size: Histogram,
    pub query_payload_error: IntCounterVec,
    pub _db_query_cost: Histogram,
    pub query_validation_latency: HistogramVec,
    pub request_response_time: Histogram,
    pub type_argument_depth: Histogram,
    pub type_argument_width: Histogram,
    pub type_nodes: Histogram,
    pub move_value_depth: Histogram,
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

const MOVE_TYPE_BUCKETS: &[f64] = &[
    1., 2., 4., 8., 12., 16., 24., 32., 48., 64., 96., 128., 256., 512., 1024.,
];

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
                &["error_rate", "payload"],
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
            query_validation_latency: register_histogram_vec_with_registry!(
                "query_validation_latency",
                "The time in ms to validate the query",
                &["validation", "latency"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            type_argument_depth: register_histogram_with_registry!(
                "type_argument_depth",
                "Number of nested type arguments in Move Types",
                MOVE_TYPE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            type_argument_width: register_histogram_with_registry!(
                "type_argument_width",
                "Number of type arguments passed into a generic instantiation of a resolved Move Type",
                MOVE_TYPE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            type_nodes: register_histogram_with_registry!(
                "type_nodes",
                "Number of structs that are processed when calculating the layout of a single Move Type",
                MOVE_TYPE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            move_value_depth: register_histogram_with_registry!(
                "move_value_depth",
                "Number of nested struct fields when calculating the layout of a single Move Type",
                MOVE_TYPE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            request_response_time: register_histogram_with_registry!(
                "request_response_time",
                "The time needed to resolve and get the result for the request",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct QueryMetrics {
    pub total_queries: IntCounterVec,
    pub total_address: IntCounterVec,
    pub total_balance: IntCounterVec,
    pub total_checkpoint: IntCounterVec,
    pub total_chain_identifier: IntCounterVec,
    pub total_coin: IntCounterVec,
    pub total_epoch: IntCounterVec,
    pub total_events: IntCounterVec,
    pub total_move_obj: IntCounterVec,
    pub total_move_package: IntCounterVec,
    pub total_move_module: IntCounterVec,
    pub total_move_function: IntCounterVec,
    pub total_object: IntCounterVec,
    pub total_protocol_configs: IntCounterVec,
    pub total_service_config: IntCounterVec,
    pub total_transactions: IntCounterVec,
}

impl QueryMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            total_queries: register_int_counter_vec_with_registry!(
                "total_queries",
                "Total number of GraphQL queries",
                &["counters"],
                registry
            )
            .unwrap(),
            total_address: register_int_counter_vec_with_registry!(
                "total_address",
                "Total number of GraphQL queries that requested address",
                &["counters"],
                registry
            )
            .unwrap(),
            total_balance: register_int_counter_vec_with_registry!(
                "total_balance",
                "Total number of GraphQL queries that requested balance",
                &["counters"],
                registry
            )
            .unwrap(),
            total_checkpoint: register_int_counter_vec_with_registry!(
                "total_checkpoint",
                "Total number of GraphQL queries that requested checkpoint",
                &["counters"],
                registry
            )
            .unwrap(),
            total_chain_identifier: register_int_counter_vec_with_registry!(
                "total_chain_identifier",
                "Total number of GraphQL queries that requested chain_identifier",
                &["counters"],
                registry
            )
            .unwrap(),
            total_coin: register_int_counter_vec_with_registry!(
                "total_coin",
                "Total number of GraphQL queries that requested coin",
                &["counters"],
                registry
            )
            .unwrap(),
            total_epoch: register_int_counter_vec_with_registry!(
                "total_epoch",
                "Total number of GraphQL queries that requested epoch",
                &["counters"],
                registry
            )
            .unwrap(),
            total_events: register_int_counter_vec_with_registry!(
                "total_events",
                "Total number of GraphQL queries that requested events",
                &["counters"],
                registry
            )
            .unwrap(),
            total_move_obj: register_int_counter_vec_with_registry!(
                "total_move_obj",
                "Total number of GraphQL queries that requested a move obj",
                &["counters"],
                registry
            )
            .unwrap(),
            total_move_package: register_int_counter_vec_with_registry!(
                "total_move_package",
                "Total number of GraphQL queries that requested a move package",
                &["counters"],
                registry
            )
            .unwrap(),
            total_move_module: register_int_counter_vec_with_registry!(
                "total_move_module",
                "Total number of GraphQL queries that requested a move module",
                &["counters"],
                registry
            )
            .unwrap(),

            total_move_function: register_int_counter_vec_with_registry!(
                "total_move_function",
                "Total number of GraphQL queries that requested a move function",
                &["counters"],
                registry
            )
            .unwrap(),
            total_object: register_int_counter_vec_with_registry!(
                "total_object",
                "Total number of GraphQL queries that requested an object",
                &["counters"],
                registry
            )
            .unwrap(),
            total_protocol_configs: register_int_counter_vec_with_registry!(
                "total_protocol_configs",
                "Total number of GraphQL queries that requested protocol configs",
                &["counters"],
                registry
            )
            .unwrap(),
            total_service_config: register_int_counter_vec_with_registry!(
                "total_service_config",
                "Total number of GraphQL queries that requested the service config",
                &["counters"],
                registry
            )
            .unwrap(),
            total_transactions: register_int_counter_vec_with_registry!(
                "total_transactions",
                "Total number of GraphQL queries that requested a transaction",
                &["counters"],
                registry
            )
            .unwrap(),
        }
    }
}
