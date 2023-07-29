// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! with_tracing {
    ($time_spent_threshold:expr, $future:expr) => {{
        use tracing::{info, error, Instrument, Span};
        use jsonrpsee::core::{RpcResult, Error as RpcError};
        use jsonrpsee::types::error::{CallError};
        use $crate::error::RpcInterimResult;
        use anyhow::anyhow;

        async move {
            let start = std::time::Instant::now();
            let interim_result: RpcInterimResult<_> = $future.await;
            let elapsed = start.elapsed();
            let result: RpcResult<_> = interim_result.map_err(|e: Error| {
                let anyhow_error = anyhow!("{:?}", e);

                let rpc_error = e.to_rpc_error();
                if !matches!(rpc_error, RpcError::Call(CallError::InvalidParams(_))) {
                    error!(error=?anyhow_error);
                }
                rpc_error
            });

            if elapsed > $time_spent_threshold {
                info!(?elapsed, "RPC took longer than threshold to complete.");
            }
            result
        }
        .instrument(Span::current())
        .await
    }};

    ($future:expr) => {{
        with_tracing!(std::time::Duration::from_secs(1), $future)
    }};
}
