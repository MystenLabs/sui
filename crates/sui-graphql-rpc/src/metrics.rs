// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::histogram::HistogramVec;
use prometheus::{
    register_counter_vec_with_registry, register_counter_with_registry,
    register_histogram_with_registry, register_int_counter_vec,
    register_int_counter_vec_with_registry, Histogram, IntCounterVec, Registry,
};

pub struct DBMetrics {
    pub(crate) db_fetch_success_rate: IntCounterVec,
    pub(crate) db_fetch_error_rate: IntCounterVec,
    pub(crate) db_fetches_latency_ms: HistogramVec,
    pub(crate) db_fetches_batch_size: HistogramVec,
}

// const LATENCY_SEC_BUCKETS: &[f64] = &[
// 0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
// ];

impl DBMetrics {
    fn new(registry: &Registry) -> Self {
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

            // not sure if we want this histogram from metrics or the prometheus one
            db_fetches_latency_ms: HistogramVec::new_in_registry(
                "db_fetches_latency_ms",
                "Latency of fetches from the db",
                &["db", "latency"],
                registry,
            ),

            db_fetches_batch_size: HistogramVec::new_in_registry(
                "db_fetches_batch_size",
                "Number of ids fetched per batch",
                &["db", "batch_size"],
                registry,
            ),
        }
    }
}

pub struct RequestMetrics {
    pub(crate) input_nodes: Histogram,
    pub(crate) output_nodes: Histogram,
    pub(crate) query_depth: Histogram,
    pub(crate) query_payload_size: Histogram,
    pub(crate) query_payload_error: IntCounterVec,
    pub(crate) _db_query_cost: Histogram,
    pub(crate) query_validation_latency: HistogramVec,
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

impl RequestMetrics {
    pub fn new(registry: &Registry) -> Self {
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
            query_validation_latency: HistogramVec::new_in_registry(
                "query_validation_latency",
                "The time in ms to validate the query",
                &["validation", "latency"],
                registry,
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueryMetrics {
    pub(crate) total_queries: IntCounterVec,
    pub(crate) total_address: IntCounterVec,
    pub(crate) total_balance: IntCounterVec,
    pub(crate) total_checkpoint: IntCounterVec,
    pub(crate) total_chain_identifier: IntCounterVec,
    pub(crate) total_coin: IntCounterVec,
    pub(crate) total_epoch: IntCounterVec,
    pub(crate) total_events: IntCounterVec,
    pub(crate) total_move_obj: IntCounterVec,
    pub(crate) total_move_package: IntCounterVec,
    pub(crate) total_move_module: IntCounterVec,
    pub(crate) total_move_function: IntCounterVec,
    pub(crate) total_object: IntCounterVec,
    pub(crate) total_protocol_configs: IntCounterVec,
    pub(crate) total_service_config: IntCounterVec,
    pub(crate) total_transactions: IntCounterVec,
}

impl QueryMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
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
        })
    }
}
