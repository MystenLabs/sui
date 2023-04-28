// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! with_tracing {
    ($method_name:literal, $future:expr) => {{
        use tracing::{info, error, Instrument, Span};
        async move {
            let result = $future.await;
            match &result {
                Ok(_) => info!("success"),
                Err(e) => error!(error = ?e, "failed"),
            }

            result
        }
        .instrument(Span::current())
        .await
    }};
}
