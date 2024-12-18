// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
pub use bridge::BridgeReadApiClient;
pub use bridge::BridgeReadApiOpenRpc;
pub use bridge::BridgeReadApiServer;
pub use coin::CoinReadApiClient;
pub use coin::CoinReadApiOpenRpc;
pub use coin::CoinReadApiServer;
pub use extended::ExtendedApiClient;
pub use extended::ExtendedApiOpenRpc;
pub use extended::ExtendedApiServer;
pub use governance::GovernanceReadApiClient;
pub use governance::GovernanceReadApiOpenRpc;
pub use governance::GovernanceReadApiServer;
pub use indexer::IndexerApiClient;
pub use indexer::IndexerApiOpenRpc;
pub use indexer::IndexerApiServer;
pub use move_utils::MoveUtilsClient;
pub use move_utils::MoveUtilsOpenRpc;
pub use move_utils::MoveUtilsServer;
use once_cell::sync::Lazy;
use prometheus::register_histogram_with_registry;
use prometheus::Histogram;
use prometheus::{register_int_counter_with_registry, IntCounter};
pub use read::ReadApiClient;
pub use read::ReadApiOpenRpc;
pub use read::ReadApiServer;
use tap::TapFallible;
use tracing::warn;
pub use transaction_builder::TransactionBuilderClient;
pub use transaction_builder::TransactionBuilderOpenRpc;
pub use transaction_builder::TransactionBuilderServer;
pub use write::WriteApiClient;
pub use write::WriteApiOpenRpc;
pub use write::WriteApiServer;

mod bridge;
mod coin;
mod extended;
mod governance;
mod indexer;
mod move_utils;
mod read;
mod transaction_builder;
mod write;

const RPC_QUERY_MAX_RESULT_LIMIT: &str = "RPC_QUERY_MAX_RESULT_LIMIT";
const DEFAULT_RPC_QUERY_MAX_RESULT_LIMIT: usize = 50;

pub static QUERY_MAX_RESULT_LIMIT: Lazy<usize> = Lazy::new(|| {
    read_size_from_env(RPC_QUERY_MAX_RESULT_LIMIT).unwrap_or(DEFAULT_RPC_QUERY_MAX_RESULT_LIMIT)
});

// TODOD(chris): make this configurable
pub const QUERY_MAX_RESULT_LIMIT_CHECKPOINTS: usize = 100;

pub fn cap_page_limit(limit: Option<usize>) -> usize {
    let limit = limit.unwrap_or_default();
    if limit > *QUERY_MAX_RESULT_LIMIT || limit == 0 {
        *QUERY_MAX_RESULT_LIMIT
    } else {
        limit
    }
}

pub fn validate_limit(limit: Option<usize>, max: usize) -> Result<usize, anyhow::Error> {
    match limit {
        Some(l) if l > max => Err(anyhow!("Page size limit {l} exceeds max limit {max}")),
        Some(0) => Err(anyhow!("Page size limit cannot be smaller than 1")),
        Some(l) => Ok(l),
        None => Ok(max),
    }
}

#[derive(Clone)]
pub struct JsonRpcMetrics {
    pub get_objects_limit: Histogram,
    pub get_objects_result_size: Histogram,
    pub get_objects_result_size_total: IntCounter,
    pub get_tx_blocks_limit: Histogram,
    pub get_tx_blocks_result_size: Histogram,
    pub get_tx_blocks_result_size_total: IntCounter,
    pub get_checkpoints_limit: Histogram,
    pub get_checkpoints_result_size: Histogram,
    pub get_checkpoints_result_size_total: IntCounter,
    pub get_owned_objects_limit: Histogram,
    pub get_owned_objects_result_size: Histogram,
    pub get_owned_objects_result_size_total: IntCounter,
    pub get_coins_limit: Histogram,
    pub get_coins_result_size: Histogram,
    pub get_coins_result_size_total: IntCounter,
    pub get_dynamic_fields_limit: Histogram,
    pub get_dynamic_fields_result_size: Histogram,
    pub get_dynamic_fields_result_size_total: IntCounter,
    pub query_tx_blocks_limit: Histogram,
    pub query_tx_blocks_result_size: Histogram,
    pub query_tx_blocks_result_size_total: IntCounter,
    pub query_events_limit: Histogram,
    pub query_events_result_size: Histogram,
    pub query_events_result_size_total: IntCounter,

    pub get_stake_sui_result_size: Histogram,
    pub get_stake_sui_result_size_total: IntCounter,

    pub get_stake_sui_latency: Histogram,
    pub get_delegated_sui_latency: Histogram,

    pub orchestrator_latency_ms: Histogram,
    pub post_orchestrator_latency_ms: Histogram,
}

impl JsonRpcMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            get_objects_limit: register_histogram_with_registry!(
                "json_rpc_get_objects_limit",
                "The input limit for multi_get_objects, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_objects_result_size: register_histogram_with_registry!(
                "json_rpc_get_objects_result_size",
                "The return size for multi_get_objects",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_objects_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_objects_result_size_total",
                "The total return size for multi_get_objects",
                registry
            )
            .unwrap(),
            get_tx_blocks_limit: register_histogram_with_registry!(
                "json_rpc_get_tx_blocks_limit",
                "The input limit for get_tx_blocks, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_tx_blocks_result_size: register_histogram_with_registry!(
                "json_rpc_get_tx_blocks_result_size",
                "The return size for get_tx_blocks",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_tx_blocks_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_tx_blocks_result_size_total",
                "The total return size for get_tx_blocks",
                registry
            )
            .unwrap(),
            get_checkpoints_limit: register_histogram_with_registry!(
                "json_rpc_get_checkpoints_limit",
                "The input limit for get_checkpoints, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_checkpoints_result_size: register_histogram_with_registry!(
                "json_rpc_get_checkpoints_result_size",
                "The return size for get_checkpoints",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_checkpoints_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_checkpoints_result_size_total",
                "The total return size for get_checkpoints",
                registry
            )
            .unwrap(),
            get_owned_objects_limit: register_histogram_with_registry!(
                "json_rpc_get_owned_objects_limit",
                "The input limit for get_owned_objects, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_owned_objects_result_size: register_histogram_with_registry!(
                "json_rpc_get_owned_objects_result_size",
                "The return size for get_owned_objects",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_owned_objects_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_owned_objects_result_size_total",
                "The total return size for get_owned_objects",
                registry
            )
            .unwrap(),
            get_coins_limit: register_histogram_with_registry!(
                "json_rpc_get_coins_limit",
                "The input limit for get_coins, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_coins_result_size: register_histogram_with_registry!(
                "json_rpc_get_coins_result_size",
                "The return size for get_coins",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_coins_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_coins_result_size_total",
                "The total return size for get_coins",
                registry
            )
            .unwrap(),
            get_dynamic_fields_limit: register_histogram_with_registry!(
                "json_rpc_get_dynamic_fields_limit",
                "The input limit for get_dynamic_fields, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_dynamic_fields_result_size: register_histogram_with_registry!(
                "json_rpc_get_dynamic_fields_result_size",
                "The return size for get_dynamic_fields",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_dynamic_fields_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_dynamic_fields_result_size_total",
                "The total return size for get_dynamic_fields",
                registry
            )
            .unwrap(),
            query_tx_blocks_limit: register_histogram_with_registry!(
                "json_rpc_query_tx_blocks_limit",
                "The input limit for query_tx_blocks, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_tx_blocks_result_size: register_histogram_with_registry!(
                "json_rpc_query_tx_blocks_result_size",
                "The return size for query_tx_blocks",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_tx_blocks_result_size_total: register_int_counter_with_registry!(
                "json_rpc_query_tx_blocks_result_size_total",
                "The total return size for query_tx_blocks",
                registry
            )
            .unwrap(),
            query_events_limit: register_histogram_with_registry!(
                "json_rpc_query_events_limit",
                "The input limit for query_events, after applying the cap",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_events_result_size: register_histogram_with_registry!(
                "json_rpc_query_events_result_size",
                "The return size for query_events",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            query_events_result_size_total: register_int_counter_with_registry!(
                "json_rpc_query_events_result_size_total",
                "The total return size for query_events",
                registry
            )
            .unwrap(),
            get_stake_sui_result_size: register_histogram_with_registry!(
                "json_rpc_get_stake_sui_result_size",
                "The return size for get_stake_sui",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_stake_sui_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_stake_sui_result_size_total",
                "The total return size for get_stake_sui",
                registry
            )
            .unwrap(),
            get_stake_sui_latency: register_histogram_with_registry!(
                "get_stake_sui_latency",
                "The latency of get stake sui, in ms",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            get_delegated_sui_latency: register_histogram_with_registry!(
                "get_delegated_sui_latency",
                "The latency of get delegated sui, in ms",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            orchestrator_latency_ms: register_histogram_with_registry!(
                "json_rpc_orchestrator_latency",
                "The latency of submitting transaction via transaction orchestrator, in ms",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            post_orchestrator_latency_ms: register_histogram_with_registry!(
                "json_rpc_post_orchestrator_latency",
                "The latency of response processing after transaction orchestrator, in ms",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

pub fn read_size_from_env(var_name: &str) -> Option<usize> {
    std::env::var(var_name)
        .ok()?
        .parse::<usize>()
        .tap_err(|e| {
            warn!(
                "Env var {} does not contain valid usize integer: {}",
                var_name, e
            )
        })
        .ok()
}

pub const CLIENT_REQUEST_METHOD_HEADER: &str = "client-request-method";
pub const CLIENT_SDK_TYPE_HEADER: &str = "client-sdk-type";
/// The version number of the SDK itself. This can be different from the API version.
pub const CLIENT_SDK_VERSION_HEADER: &str = "client-sdk-version";
/// The RPC API version that the client is targeting. Different SDK versions may target the same
/// API version.
pub const CLIENT_TARGET_API_VERSION_HEADER: &str = "client-target-api-version";

pub const TRANSIENT_ERROR_CODE: i32 = -32050;
pub const TRANSACTION_EXECUTION_CLIENT_ERROR_CODE: i32 = -32002;
