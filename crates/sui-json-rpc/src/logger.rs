// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! with_tracing {
    ($future:expr) => {{
        use tracing::{info, error, Instrument, Span};
        use jsonrpsee::core::RpcResult;

        async move {
            let result: RpcResult<_> = $future.await;

            match &result {
                Ok(_) => info!("success"),
                Err(e) => error!(error = ?e, error_code = e.code())
            }
            result
        }
        .instrument(Span::current())
        .await
    }};
}
