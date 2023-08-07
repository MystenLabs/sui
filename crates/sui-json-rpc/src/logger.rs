// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! with_tracing {
    ($future:expr) => {{
        use tracing::{info, error, Instrument, Span};
        use jsonrpsee::core::{RpcResult, Error as RpcError};
        use jsonrpsee::types::error::{CallError, INVALID_PARAMS_CODE, CALL_EXECUTION_FAILED_CODE};

        async move {
            let result: RpcResult<_> = $future.await;

            match &result {
                Ok(_) => info!("success"),
                Err(e) => {
                    match e {
                        RpcError::Call(call_error) => {
                            let error_code = match call_error {
                                CallError::InvalidParams(_) => INVALID_PARAMS_CODE,
                                _ => CALL_EXECUTION_FAILED_CODE
                            };
                            error!(error = ?e, error_code = error_code);
                        }
                        _ => error!(error = ?e),
                    }
                }
            }
            result
        }
        .instrument(Span::current())
        .await
    }};
}
