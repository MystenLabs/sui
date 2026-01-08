// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::convert::Infallible;
use std::fmt::Display;
use std::sync::Arc;

use axum::Json;
use axum::response::IntoResponse;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::types::error::INTERNAL_ERROR_CODE;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use serde_json::json;
use tower_http::catch_panic::ResponseForPanic;

use crate::metrics::RpcMetrics;

/// Request timed out.
pub const TIMEOUT_ERROR_CODE: i32 = -32604;

/// Like anyhow's `bail!`, but for returning an internal error.
macro_rules! rpc_bail {
    ($($arg:tt)*) => {
        return Err(crate::error::internal_error!($($arg)*))
    };
}

/// Like anyhow's `anyhow!`, but for returning an internal error.
macro_rules! internal_error {
    ($($arg:tt)*) => {
        crate::error::RpcError::InternalError(anyhow::anyhow!($($arg)*))
    };
}

pub(crate) use internal_error;
pub(crate) use rpc_bail;

/// Behaves exactly like `anyhow::Context`, but only adds context to `RpcError::InternalError`.
pub(crate) trait InternalContext<T, E: std::error::Error> {
    fn internal_context<C>(self, ctx: C) -> Result<T, RpcError<E>>
    where
        C: Display + Send + Sync + 'static;

    fn with_internal_context<C, F>(self, f: F) -> Result<T, RpcError<E>>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

/// This type represents three kinds of errors: Invalid Params (the user's fault), Timeouts, and
/// Internal Errors (the service's fault). Each RpcModule is responsible for defining its own
/// structured user errors, while timeouts and internal errors are represented with anyhow
/// everywhere.
///
/// The internal error type defaults to `Infallible`, meaning there are no reasons the response
/// might fail because of user input.
///
/// This representation was chosen to encourage a pattern where errors that are presented to users
/// have a single source of truth for how they should be displayed, while internal errors encourage
/// the addition of context (extra information to build a trace of why something went wrong).
///
/// User errors must be explicitly wrapped with `invalid_params` while internal errors are
/// implicitly converted using the `?` operator. This asymmetry comes from the fact that we could
/// populate `E` with `anyhow::Error`, which would then cause `From` impls to overlap if we
/// supported conversion from both `E` and `anyhow::Error`.
#[derive(thiserror::Error, Debug)]
pub(crate) enum RpcError<E: std::error::Error = Infallible> {
    #[error("Invalid Params: {0}")]
    InvalidParams(E),

    #[error("Timed out: {0}")]
    Timeout(anyhow::Error),

    #[error("Internal Error: {0:#}")]
    InternalError(#[from] anyhow::Error),
}

/// Handler for panics that occur during request processing. Converts panics into JSON-RPC error
/// responses with a 500 status code.
#[derive(Clone)]
pub(crate) struct PanicHandler {
    metrics: Arc<RpcMetrics>,
}

impl PanicHandler {
    pub fn new(metrics: Arc<RpcMetrics>) -> Self {
        Self { metrics }
    }
}

impl<T, E: std::error::Error> InternalContext<T, E> for Result<T, RpcError<E>> {
    /// Wrap an internal error with additional context.
    fn internal_context<C>(self, ctx: C) -> Result<T, RpcError<E>>
    where
        C: Display + Send + Sync + 'static,
    {
        use RpcError as E;
        match self {
            Err(E::InternalError(e)) => Err(E::InternalError(e.context(ctx))),
            Err(E::Timeout(e)) => Err(E::Timeout(e.context(ctx))),
            _ => self,
        }
    }

    /// Wrap an internal error with additional context that is lazily evaluated only once an
    /// internal error has occured.
    fn with_internal_context<C, F>(self, f: F) -> Result<T, RpcError<E>>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        use RpcError as E;
        match self {
            Err(E::InternalError(e)) => Err(E::InternalError(e.context(f()))),
            Err(E::Timeout(e)) => Err(E::Timeout(e.context(f()))),
            _ => self,
        }
    }
}

impl<E: std::error::Error> From<RpcError<E>> for ErrorObject<'static> {
    fn from(err: RpcError<E>) -> Self {
        use RpcError as E;
        match &err {
            E::InvalidParams(_) => {
                ErrorObject::owned(INVALID_PARAMS_CODE, err.to_string(), None::<()>)
            }

            E::Timeout(_) => ErrorObject::owned(TIMEOUT_ERROR_CODE, err.to_string(), None::<()>),

            E::InternalError(_) => {
                ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>)
            }
        }
    }
}

/// Helper function to convert a user error into the `RpcError` type.
pub(crate) fn invalid_params<E: std::error::Error>(err: E) -> RpcError<E> {
    RpcError::InvalidParams(err)
}

/// Helper function to convert a jsonrpc client error into an `ErrorObject`.
pub(crate) fn client_error_to_error_object(
    error: jsonrpsee::core::ClientError,
) -> ErrorObject<'static> {
    match error {
        // `Call` is the only error type that actually conveys meaningful error
        // from a user calling the method. Other error variants are all more or less
        // internal errors.
        jsonrpsee::core::ClientError::Call(e) => e,
        _ => ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>),
    }
}

impl ResponseForPanic for PanicHandler {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(
        &mut self,
        err: Box<dyn Any + Send + 'static>,
    ) -> axum::http::Response<Self::ResponseBody> {
        self.metrics.requests_panicked.inc();

        let err = if let Some(s) = err.downcast_ref::<String>() {
            anyhow::anyhow!(s.clone()).context("Request panicked")
        } else if let Some(s) = err.downcast_ref::<&str>() {
            anyhow::anyhow!(s.to_string()).context("Request panicked")
        } else {
            anyhow::anyhow!("Request panicked")
        };

        let err: RpcError = err.into();
        let err: ErrorObject<'static> = err.into();

        let resp = json!({
            "jsonrpc": "2.0",
            "error": err,
            "id": null,
        });

        Json(resp).into_response()
    }
}
