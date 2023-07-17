// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! with_tracing {
    ($time_spent_threshold:expr, $future:expr) => {{
        use tracing::{info, error, Instrument, Span};
        use jsonrpsee::core::{RpcResult, Error as RpcError};
        use jsonrpsee::types::error::{CallError, CALL_EXECUTION_FAILED_CODE};

        async move {
            let start = std::time::Instant::now();
            let result: RpcResult<_> = $future.await;
            let elapsed = start.elapsed();
            if let Err(e) = &result {
                match e {
                    RpcError::Call(call_error) => {
                        match call_error {
                            // We don't log user input errors
                            CallError::InvalidParams(_) => (),
                            _ => error!(error = ?e, error_code = CALL_EXECUTION_FAILED_CODE)
                        };
                    }
                    _ => error!(error = ?e),
                }
            }
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
