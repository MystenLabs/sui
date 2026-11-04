// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Admission: the view of the ledger each request is served at.
//!
//! [`AdmissionLayer`] reads this replica's cached service info once per request, as it admits the
//! request, and records it in the request's extensions as [`AdmittedServiceInfo`]. Every handler
//! bounds its answer by that recorded view (see [`crate::served_height::ServedHeight::of`]) rather
//! than by the live cache, so a refresh that lands while a request is in flight does not change
//! what the request reads.
//!
//! # Monotonic reads across load-balanced replicas
//!
//! Every replica keeps its own view of how far the ledger has been indexed. Behind a load balancer
//! those views differ, so a client whose requests land on different replicas can be served from a
//! view older than one it has already been shown, and sees time run backwards.
//!
//! A client avoids that by stamping the checkpoint it has already observed on each request.
//! [`AdmissionLayer`] holds the request until this replica's view reaches that checkpoint, admits it
//! at the view that reached it, and answers [`tonic::Code::Unavailable`] if none does within its
//! timeout. Requests without the header are admitted at whatever view the replica holds, so clients
//! that do not need monotonic reads are unaffected.
//!
//! The header is a floor on the view, not a request to read *as of* that checkpoint: an answer may
//! be newer, never older. The layer applies it to every method rather than only those whose answer
//! is bounded by the watermark, so the guarantee a caller gets does not depend on which method they
//! called.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use http::HeaderMap;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc_api::RpcError;
use tokio::sync::watch;
use tonic::Status;
use tower::Layer;
use tower::Service;

/// Checkpoint the client has already observed, as decimal ASCII. A server serves the request only
/// once it can serve data up to at least this checkpoint.
pub const X_SUI_MIN_CHECKPOINT: &str = "x-sui-min-checkpoint";

/// The view of the ledger a request is served at: this replica's cached service info when
/// [`AdmissionLayer`] admitted the request, `None` if the replica had none yet.
#[derive(Clone, Debug)]
pub(crate) struct AdmittedServiceInfo(Option<Arc<GetServiceInfoResponse>>);

impl AdmittedServiceInfo {
    /// The view `request` was admitted at. `Internal` for a request that did not pass through
    /// [`AdmissionLayer`].
    pub(crate) fn of<T>(request: &tonic::Request<T>) -> Result<&Self, RpcError> {
        request.extensions().get().ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "request reached its handler without being admitted",
            )
        })
    }

    /// The service info this request is served at, `None` if the replica had none when it
    /// admitted the request.
    pub(crate) fn service_info(&self) -> Option<&GetServiceInfoResponse> {
        self.0.as_deref()
    }
}

/// Admits every request reaching the services below it: holds a request that asks for a monotonic
/// read until this replica can serve it, and records the view each request is served at.
///
/// `service_info` carries this replica's view of the ledger, `None` before it has one, and its
/// `checkpoint_height` is the highest checkpoint the replica can serve. Its owner must publish
/// every change to that view; a waiter re-evaluates on each publish.
#[derive(Clone)]
pub(crate) struct AdmissionLayer {
    service_info: watch::Receiver<Option<Arc<GetServiceInfoResponse>>>,
    timeout: Duration,
}

impl AdmissionLayer {
    pub(crate) fn new(
        service_info: watch::Receiver<Option<Arc<GetServiceInfoResponse>>>,
        timeout: Duration,
    ) -> Self {
        Self {
            service_info,
            timeout,
        }
    }
}

impl<S> Layer<S> for AdmissionLayer {
    type Service = Admission<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Admission {
            inner,
            service_info: self.service_info.clone(),
            timeout: self.timeout,
        }
    }
}

/// See [`AdmissionLayer`].
#[derive(Clone)]
pub(crate) struct Admission<S> {
    inner: S,
    service_info: watch::Receiver<Option<Arc<GetServiceInfoResponse>>>,
    timeout: Duration,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for Admission<S>
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

    fn call(&mut self, mut request: http::Request<ReqBody>) -> Self::Future {
        // Only the service `poll_ready` readied may be called, so move that one into the future
        // and leave a fresh clone behind to be readied for the next request. See
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let not_ready_inner = self.inner.clone();
        let mut ready_inner = std::mem::replace(&mut self.inner, not_ready_inner);
        let mut service_info = self.service_info.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            match admit(&mut service_info, timeout, request.headers()).await {
                Ok(admitted) => {
                    request.extensions_mut().insert(admitted);
                    ready_inner.call(request).await
                }
                Err(status) => Ok(status.into_http()),
            }
        })
    }
}

/// The view a request with `headers` is served at. Without a checkpoint to wait for, that is the
/// replica's current view. With one, it is the first view at or above that checkpoint, waiting up
/// to `timeout` for the replica to reach it.
async fn admit(
    service_info: &mut watch::Receiver<Option<Arc<GetServiceInfoResponse>>>,
    timeout: Duration,
    headers: &HeaderMap,
) -> Result<AdmittedServiceInfo, Status> {
    let Some(checkpoint) = extract_min_checkpoint(headers)? else {
        return Ok(AdmittedServiceInfo(service_info.borrow().clone()));
    };

    let catch_up = async {
        service_info
            .wait_for(|info| {
                info.as_ref()
                    .and_then(|info| info.checkpoint_height)
                    .is_some_and(|height| height >= checkpoint)
            })
            .await
            .map(|info| info.clone())
    };

    match tokio::time::timeout(timeout, catch_up).await {
        Ok(Ok(info)) => Ok(AdmittedServiceInfo(info)),
        Ok(Err(_)) | Err(_) => Err(Status::unavailable(format!(
            "monotonic read at checkpoint {checkpoint} not available on this replica; retry"
        ))),
    }
}

/// The checkpoint `headers` asks the server to have reached, or `None` when the request does not
/// ask for a monotonic read.
fn extract_min_checkpoint(headers: &HeaderMap) -> Result<Option<u64>, Status> {
    let Some(value) = headers.get(X_SUI_MIN_CHECKPOINT) else {
        return Ok(None);
    };

    // A header is not a request-body field, so this carries no `BadRequest` detail, whose `field`
    // must name a protobuf field path.
    let invalid =
        |detail: String| Status::invalid_argument(format!("{X_SUI_MIN_CHECKPOINT}: {detail}"));

    let value = value
        .to_str()
        .map_err(|e| invalid(format!("header is not valid ASCII: {e}")))?;

    value
        .parse()
        .map(Some)
        .map_err(|e| invalid(format!("invalid checkpoint {value:?}: {e}")))
}

#[cfg(test)]
impl crate::KvRpcServer {
    /// `message` as a request admitted at this replica's current view, as [`AdmissionLayer`]
    /// admits a request without [`X_SUI_MIN_CHECKPOINT`].
    pub(crate) fn admitted_request<T>(&self, message: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(message);
        request
            .extensions_mut()
            .insert(AdmittedServiceInfo(self.cache.borrow().clone()));
        request
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use tower::ServiceExt;
    use tower::service_fn;

    use super::*;

    fn info_at(checkpoint: u64) -> Option<Arc<GetServiceInfoResponse>> {
        let mut info = GetServiceInfoResponse::default();
        info.checkpoint_height = Some(checkpoint);
        Some(Arc::new(info))
    }

    #[tokio::test]
    async fn monotonic_read_is_admitted_at_the_height_that_reached_its_floor() {
        let (publish, service_info) = watch::channel(info_at(4));
        // Answers with the checkpoint height its request was admitted at.
        let admission = AdmissionLayer::new(service_info, Duration::from_secs(5)).layer(
            service_fn(|request: http::Request<()>| async move {
                let admitted = request
                    .extensions()
                    .get::<AdmittedServiceInfo>()
                    .and_then(AdmittedServiceInfo::service_info)
                    .and_then(|info| info.checkpoint_height);
                Ok::<_, Infallible>(http::Response::new(admitted))
            }),
        );
        let mut request = http::Request::new(());
        request.headers_mut().insert(X_SUI_MIN_CHECKPOINT, 5.into());
        let mut response = Box::pin(admission.oneshot(request));

        tokio::time::timeout(Duration::from_millis(50), &mut response)
            .await
            .expect_err("held while the replica is below the floor");
        publish.send_replace(info_at(5));
        let response = response.await.unwrap();
        assert_eq!(*response.body(), Some(5));
    }
}
