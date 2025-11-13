// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use anyhow::anyhow;
use jsonrpsee::{MethodResponse, server::middleware::rpc::RpcServiceT, types::Request};
use pin_project_lite::pin_project;
use tokio::time::{Timeout, timeout};
use tower::Layer;
use tracing::warn;

use crate::error::RpcError;

/// Tower Layer that adds middleware to timeout requests after a given duration. The method name
/// and parameters will be logged for requests that time out.
#[derive(Clone)]
pub(crate) struct TimeoutLayer {
    request_timeout: Duration,
}

/// The Tower Service responsible for wrapping the JSON-RPC request handler with timeout handling.
pub(crate) struct TimeoutService<S> {
    request_timeout: Duration,
    inner: S,
}

pin_project! {
    pub(crate) struct TimeoutFuture<'a, F> {
        request: Request<'a>,
        #[pin] inner: Timeout<F>
    }
}

impl TimeoutLayer {
    /// Create a new timeout layer that fails requests after `request_timeout`.
    pub fn new(request_timeout: Duration) -> Self {
        Self { request_timeout }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = TimeoutService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimeoutService {
            request_timeout: self.request_timeout,
            inner,
        }
    }
}

impl<'a, S> RpcServiceT<'a> for TimeoutService<S>
where
    S: RpcServiceT<'a>,
{
    type Future = TimeoutFuture<'a, S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        TimeoutFuture {
            request: request.clone(),
            inner: timeout(self.request_timeout, self.inner.call(request)),
        }
    }
}

impl<F> Future for TimeoutFuture<'_, F>
where
    F: Future<Output = MethodResponse>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let Poll::Ready(resp) = this.inner.poll(cx) else {
            return Poll::Pending;
        };

        let Ok(resp) = resp else {
            let method = this.request.method.as_ref();
            let params = this
                .request
                .params
                .as_ref()
                .map(|p| p.get())
                .unwrap_or("[]");

            warn!(method, params, "Request timed out");
            return Poll::Ready(MethodResponse::error(
                this.request.id.clone(),
                RpcError::<Infallible>::Timeout(anyhow!("Request timed out")),
            ));
        };

        Poll::Ready(resp)
    }
}
