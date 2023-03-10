// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod coin;
mod event;
mod governance;
mod read;
mod transaction_builder;
mod write;

pub use coin::CoinReadApiClient;
pub use coin::CoinReadApiOpenRpc;
pub use coin::CoinReadApiServer;

pub use event::EventReadApiClient;
pub use event::EventReadApiOpenRpc;
pub use event::EventReadApiServer;

pub use write::WriteApiClient;
pub use write::WriteApiOpenRpc;
pub use write::WriteApiServer;

pub use governance::GovernanceReadApiClient;
pub use governance::GovernanceReadApiOpenRpc;
pub use governance::GovernanceReadApiServer;

pub use read::ReadApiClient;
pub use read::ReadApiOpenRpc;
pub use read::ReadApiServer;

pub use transaction_builder::TransactionBuilderClient;
pub use transaction_builder::TransactionBuilderOpenRpc;
pub use transaction_builder::TransactionBuilderServer;

/// Maximum number of events returned in an event query.
/// This is equivalent to EVENT_QUERY_MAX_LIMIT in `sui-storage` crate.
/// To avoid unnecessary dependency on that crate, we have a reference here
/// for document purposes.
pub const QUERY_MAX_RESULT_LIMIT: usize = 1000;

pub fn cap_page_limit(limit: Option<usize>) -> usize {
    let limit = limit.unwrap_or_default();
    if limit > QUERY_MAX_RESULT_LIMIT || limit == 0 {
        QUERY_MAX_RESULT_LIMIT
    } else {
        limit
    }
}
