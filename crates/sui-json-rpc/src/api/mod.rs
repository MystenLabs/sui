// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use mysten_metrics::histogram::Histogram;

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
use prometheus::{register_int_counter_with_registry, IntCounter};
pub use read::ReadApiClient;
pub use read::ReadApiOpenRpc;
pub use read::ReadApiServer;
pub use transaction_builder::TransactionBuilderClient;
pub use transaction_builder::TransactionBuilderOpenRpc;
pub use transaction_builder::TransactionBuilderServer;
pub use write::WriteApiClient;
pub use write::WriteApiOpenRpc;
pub use write::WriteApiServer;

mod coin;
mod extended;
mod governance;
mod indexer;
mod move_utils;
mod read;
mod transaction_builder;
mod write;

pub const QUERY_MAX_RESULT_LIMIT: usize = 50;
// TODOD(chris): make this configurable
pub const QUERY_MAX_RESULT_LIMIT_CHECKPOINTS: usize = 100;

pub const QUERY_MAX_RESULT_LIMIT_OBJECTS: usize = 50;

pub fn cap_page_limit(limit: Option<usize>) -> usize {
    let limit = limit.unwrap_or_default();
    if limit > QUERY_MAX_RESULT_LIMIT || limit == 0 {
        QUERY_MAX_RESULT_LIMIT
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
}

impl JsonRpcMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            get_objects_limit: Histogram::new_in_registry(
                "json_rpc_get_objects_limit",
                "The input limit for multi_get_objects, after applying the cap",
                registry,
            ),
            get_objects_result_size: Histogram::new_in_registry(
                "json_rpc_get_objects_result_size",
                "The return size for multi_get_objects",
                registry,
            ),
            get_objects_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_objects_result_size_total",
                "The total return size for multi_get_objects",
                registry
            )
            .unwrap(),
            get_tx_blocks_limit: Histogram::new_in_registry(
                "json_rpc_get_tx_blocks_limit",
                "The input limit for get_tx_blocks, after applying the cap",
                registry,
            ),
            get_tx_blocks_result_size: Histogram::new_in_registry(
                "json_rpc_get_tx_blocks_result_size",
                "The return size for get_tx_blocks",
                registry,
            ),
            get_tx_blocks_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_tx_blocks_result_size_total",
                "The total return size for get_tx_blocks",
                registry
            )
            .unwrap(),
            get_checkpoints_limit: Histogram::new_in_registry(
                "json_rpc_get_checkpoints_limit",
                "The input limit for get_checkpoints, after applying the cap",
                registry,
            ),
            get_checkpoints_result_size: Histogram::new_in_registry(
                "json_rpc_get_checkpoints_result_size",
                "The return size for get_checkpoints",
                registry,
            ),
            get_checkpoints_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_checkpoints_result_size_total",
                "The total return size for get_checkpoints",
                registry
            )
            .unwrap(),
            get_owned_objects_limit: Histogram::new_in_registry(
                "json_rpc_get_owned_objects_limit",
                "The input limit for get_owned_objects, after applying the cap",
                registry,
            ),
            get_owned_objects_result_size: Histogram::new_in_registry(
                "json_rpc_get_owned_objects_result_size",
                "The return size for get_owned_objects",
                registry,
            ),
            get_owned_objects_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_owned_objects_result_size_total",
                "The total return size for get_owned_objects",
                registry
            )
            .unwrap(),
            get_coins_limit: Histogram::new_in_registry(
                "json_rpc_get_coins_limit",
                "The input limit for get_coins, after applying the cap",
                registry,
            ),
            get_coins_result_size: Histogram::new_in_registry(
                "json_rpc_get_coins_result_size",
                "The return size for get_coins",
                registry,
            ),
            get_coins_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_coins_result_size_total",
                "The total return size for get_coins",
                registry
            )
            .unwrap(),
            get_dynamic_fields_limit: Histogram::new_in_registry(
                "json_rpc_get_dynamic_fields_limit",
                "The input limit for get_dynamic_fields, after applying the cap",
                registry,
            ),
            get_dynamic_fields_result_size: Histogram::new_in_registry(
                "json_rpc_get_dynamic_fields_result_size",
                "The return size for get_dynamic_fields",
                registry,
            ),
            get_dynamic_fields_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_dynamic_fields_result_size_total",
                "The total return size for get_dynamic_fields",
                registry
            )
            .unwrap(),
            query_tx_blocks_limit: Histogram::new_in_registry(
                "json_rpc_query_tx_blocks_limit",
                "The input limit for query_tx_blocks, after applying the cap",
                registry,
            ),
            query_tx_blocks_result_size: Histogram::new_in_registry(
                "json_rpc_query_tx_blocks_result_size",
                "The return size for query_tx_blocks",
                registry,
            ),
            query_tx_blocks_result_size_total: register_int_counter_with_registry!(
                "json_rpc_query_tx_blocks_result_size_total",
                "The total return size for query_tx_blocks",
                registry
            )
            .unwrap(),
            query_events_limit: Histogram::new_in_registry(
                "json_rpc_query_events_limit",
                "The input limit for query_events, after applying the cap",
                registry,
            ),
            query_events_result_size: Histogram::new_in_registry(
                "json_rpc_query_events_result_size",
                "The return size for query_events",
                registry,
            ),
            query_events_result_size_total: register_int_counter_with_registry!(
                "json_rpc_query_events_result_size_total",
                "The total return size for query_events",
                registry
            )
            .unwrap(),
            get_stake_sui_result_size: Histogram::new_in_registry(
                "json_rpc_get_stake_sui_result_size",
                "The return size for get_stake_sui",
                registry,
            ),
            get_stake_sui_result_size_total: register_int_counter_with_registry!(
                "json_rpc_get_stake_sui_result_size_total",
                "The total return size for get_stake_sui",
                registry
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}
