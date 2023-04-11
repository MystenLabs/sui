// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

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

/// Maximum number of events returned in an event query.
/// This is equivalent to EVENT_QUERY_MAX_LIMIT in `sui-storage` crate.
/// To avoid unnecessary dependency on that crate, we have a reference here
/// for document purposes.
pub const QUERY_MAX_RESULT_LIMIT: usize = 1000;
// TODOD(chris): make this configurable
pub const QUERY_MAX_RESULT_LIMIT_CHECKPOINTS: usize = 100;

pub const QUERY_MAX_RESULT_LIMIT_OBJECTS: usize = 256;

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
