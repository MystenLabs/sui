// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod abi;
pub mod action_executor;
pub mod client;
pub mod config;
pub mod crypto;
pub mod error;
pub mod eth_client;
pub mod eth_syncer;
pub mod events;
pub mod orchestrator;
pub mod server;
pub mod storage;
pub mod sui_client;
pub mod sui_syncer;
pub mod sui_transaction_builder;
pub mod types;

#[cfg(test)]
pub(crate) mod eth_mock_provider;

#[cfg(test)]
pub(crate) mod sui_mock_client;

#[cfg(test)]
pub(crate) mod test_utils;

// TODO: can we log the error very time it gets retried?
#[macro_export]
macro_rules! retry_with_max_delay {
    ($func:expr, $max_delay:expr) => {{
        let retry_strategy = ExponentialBackoff::from_millis(100)
            .max_delay($max_delay)
            .map(jitter);
        Retry::spawn(retry_strategy, || $func).await
    }};
}
