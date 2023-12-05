// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod abi;
pub mod error;
pub mod eth_client;
pub mod eth_syncer;
pub mod events;
pub mod handler;
pub mod server;
pub mod sui_client;
pub mod sui_syncer;

#[cfg(test)]
pub(crate) mod eth_mock_provider;

#[cfg(test)]
pub(crate) mod sui_mock_client;

#[macro_export]
macro_rules! retry_with_max_delay {
    ($func:expr, $max_delay:expr) => {{
        let retry_strategy = ExponentialBackoff::from_millis(100)
            .max_delay($max_delay)
            .map(jitter);
        Retry::spawn(retry_strategy, || $func).await
    }};
}
