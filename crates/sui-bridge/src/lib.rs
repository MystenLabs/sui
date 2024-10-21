// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod abi;
pub mod action_executor;
pub mod client;
pub mod config;
pub mod crypto;
pub mod encoding;
pub mod error;
pub mod eth_client;
pub mod eth_syncer;
pub mod eth_transaction_builder;
pub mod events;
pub mod metered_eth_provider;
pub mod metrics;
pub mod monitor;
pub mod node;
pub mod orchestrator;
pub mod server;
pub mod storage;
pub mod sui_bridge_watchdog;
pub mod sui_client;
pub mod sui_syncer;
pub mod sui_transaction_builder;
pub mod types;
pub mod utils;

#[cfg(any(feature = "test-utils", test))]
pub(crate) mod eth_mock_provider;

#[cfg(test)]
pub(crate) mod sui_mock_client;

#[cfg(any(feature = "test-utils", test))]
pub mod test_utils;

pub const BRIDGE_ENABLE_PROTOCOL_VERSION: u64 = 45;

#[cfg(any(feature = "test-utils", test))]
pub mod e2e_tests;

#[macro_export]
macro_rules! retry_with_max_elapsed_time {
    ($func:expr, $max_elapsed_time:expr) => {{
        // The following delay sequence (in secs) will be used, applied with jitter
        // 0.4, 0.8, 1.6, 3.2, 6.4, 12.8, 25.6, 30, 60, 120, 120 ...
        let backoff = backoff::ExponentialBackoff {
            initial_interval: Duration::from_millis(400),
            randomization_factor: 0.1,
            multiplier: 2.0,
            max_interval: Duration::from_secs(120),
            max_elapsed_time: Some($max_elapsed_time),
            ..Default::default()
        };
        backoff::future::retry(backoff, || {
            let fut = async {
                let result = $func.await;
                match result {
                    Ok(_) => {
                        return Ok(result);
                    }
                    Err(e) => {
                        // For simplicity we treat every error as transient so we can retry until max_elapsed_time
                        tracing::debug!("Retrying due to error: {:?}", e);
                        return Err(backoff::Error::transient(e));
                    }
                }
            };
            std::boxed::Box::pin(fut)
        })
        .await
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn example_func_ok() -> anyhow::Result<()> {
        Ok(())
    }

    async fn example_func_err() -> anyhow::Result<()> {
        tracing::info!("example_func_err");
        Err(anyhow::anyhow!(""))
    }

    #[tokio::test]
    async fn test_retry_with_max_elapsed_time() {
        telemetry_subscribers::init_for_testing();
        // no retry is needed, should return immediately. We give it a very small
        // max_elapsed_time and it should still finish in time.
        let max_elapsed_time = Duration::from_millis(20);
        retry_with_max_elapsed_time!(example_func_ok(), max_elapsed_time)
            .unwrap()
            .unwrap();

        // now call a function that always errors and expect it to return before max_elapsed_time runs out
        let max_elapsed_time = Duration::from_secs(10);
        let instant = std::time::Instant::now();
        retry_with_max_elapsed_time!(example_func_err(), max_elapsed_time).unwrap_err();
        assert!(instant.elapsed() < max_elapsed_time);
    }
}
