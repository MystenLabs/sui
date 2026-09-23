// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Consistent reads across load-balanced replicas.
//!
//! Every replica keeps its own view of how far the ledger has been indexed. Behind a load balancer
//! those views differ, so a client whose requests land on different replicas can be served from a
//! view older than one it has already been shown, and sees time run backwards.
//!
//! A client avoids that by stamping the checkpoint it has already observed on each request.
//! [`ConsistentReadLayer`] holds the request until this replica's view reaches that checkpoint, and
//! answers [`tonic::Code::Unavailable`] if it cannot within its timeout. Requests without the
//! header are served at whatever view the replica holds, so clients that do not need monotonic
//! reads are unaffected.
//!
//! The header is a floor on the view, not a request to read *as of* that checkpoint: an answer may
//! be newer, never older. The layer applies it to every method rather than only those whose answer
//! is bounded by the watermark, so the guarantee a caller gets does not depend on which method they
//! called.

use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use http::HeaderMap;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use tokio::sync::watch;
use tonic::Status;
use tower::Layer;
use tower::Service;

/// Checkpoint the client has already observed, as decimal ASCII. A server serves the request only
/// once it can serve data up to at least this checkpoint.
pub const X_SUI_CONSISTENT_READ_CHECKPOINT: &str = "x-sui-consistent-read-checkpoint";

/// Applies the consistent-read contract to every request reaching the services below it.
///
/// `service_info` carries this replica's view of the ledger, `None` before it has one, and its
/// `checkpoint_height` is the highest checkpoint the replica can serve. Its owner must publish on
/// every refresh, including refreshes that do not advance the height, so a waiter re-evaluates
/// rather than sleeping until the next advance.
#[derive(Clone)]
pub(crate) struct ConsistentReadLayer {
    service_info: watch::Receiver<Option<GetServiceInfoResponse>>,
    timeout: Duration,
}

impl ConsistentReadLayer {
    pub(crate) fn new(
        service_info: watch::Receiver<Option<GetServiceInfoResponse>>,
        timeout: Duration,
    ) -> Self {
        Self {
            service_info,
            timeout,
        }
    }
}

impl<S> Layer<S> for ConsistentReadLayer {
    type Service = ConsistentRead<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ConsistentRead {
            inner,
            service_info: self.service_info.clone(),
            timeout: self.timeout,
        }
    }
}

/// See [`ConsistentReadLayer`].
#[derive(Clone)]
pub(crate) struct ConsistentRead<S> {
    inner: S,
    service_info: watch::Receiver<Option<GetServiceInfoResponse>>,
    timeout: Duration,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for ConsistentRead<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Default,
{
    type Response = http::Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<ReqBody>) -> Self::Future {
        // Only the service `poll_ready` readied may be called, so move that one into the future
        // and leave a fresh clone behind to be readied for the next request. See
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let not_ready_inner = self.inner.clone();
        let mut ready_inner = std::mem::replace(&mut self.inner, not_ready_inner);
        let mut service_info = self.service_info.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            match await_checkpoint(&mut service_info, timeout, request.headers()).await {
                Ok(()) => ready_inner.call(request).await,
                Err(status) => Ok(status.into_http()),
            }
        })
    }
}

/// Wait for this replica's height to reach the checkpoint `headers` asks for. Returns immediately
/// when the request asks for nothing, or when the replica has already reached it.
async fn await_checkpoint(
    service_info: &mut watch::Receiver<Option<GetServiceInfoResponse>>,
    timeout: Duration,
    headers: &HeaderMap,
) -> Result<(), Status> {
    let Some(checkpoint) = extract_consistent_read_checkpoint(headers)? else {
        return Ok(());
    };

    let catch_up = service_info.wait_for(|info| {
        info.as_ref()
            .and_then(|info| info.checkpoint_height)
            .is_some_and(|height| height >= checkpoint)
    });

    let reached = tokio::time::timeout(timeout, catch_up)
        .await
        .is_ok_and(|reached| reached.is_ok());

    if reached {
        return Ok(());
    }

    Err(Status::unavailable(format!(
        "consistent read at checkpoint {checkpoint} not available on this replica; retry"
    )))
}

/// The checkpoint `headers` asks the server to have reached, or `None` when the request does not
/// ask for a consistent read.
fn extract_consistent_read_checkpoint(headers: &HeaderMap) -> Result<Option<u64>, Status> {
    let Some(value) = headers.get(X_SUI_CONSISTENT_READ_CHECKPOINT) else {
        return Ok(None);
    };

    // A header is not a request-body field, so this carries no `BadRequest` detail, whose `field`
    // must name a protobuf field path.
    let invalid = |detail: String| {
        Status::invalid_argument(format!("{X_SUI_CONSISTENT_READ_CHECKPOINT}: {detail}"))
    };

    let value = value
        .to_str()
        .map_err(|e| invalid(format!("header is not valid ASCII: {e}")))?;

    value
        .parse()
        .map(Some)
        .map_err(|e| invalid(format!("invalid checkpoint {value:?}: {e}")))
}
