// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Monotonic reads against a load-balanced ledger service.
//!
//! Replicas behind a load balancer each index the ledger at their own pace, so consecutive
//! requests can be answered from views at different checkpoints. A caller that has already
//! observed a checkpoint raises a [`CheckpointFloor`], and the ledger readers built with it stamp
//! it on every request as [`X_SUI_MIN_CHECKPOINT`]. A replica that has not reached the floor holds
//! the request briefly and then answers `Unavailable`; the readers resend it so a load balancer
//! that spreads requests across replicas can route the next attempt to one that has.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

use futures::future::BoxFuture;
use http::HeaderValue;
use http_body_util::BodyExt;
use http_body_util::Full;
use tokio::sync::Mutex;
use tonic::body::Body;
use tower::Layer;
use tower::Service;
use tower::ServiceExt;

/// Header carrying the lowest checkpoint a replica must be able to serve before it answers the
/// request, as decimal ASCII. The server side is `sui_kv_rpc::monotonic_read`.
pub const X_SUI_MIN_CHECKPOINT: &str = "x-sui-min-checkpoint";

/// Total attempts for a request that keeps being answered `Unavailable`.
const MAX_UNAVAILABLE_ATTEMPTS: usize = 3;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The highest ledger checkpoint the caller has observed. It only rises, so every request stamped
/// from it asks for a view at least as new as any the caller has already been shown.
#[derive(Clone, Default, Debug)]
pub struct CheckpointFloor(Arc<AtomicU64>);

/// Stamps [`X_SUI_MIN_CHECKPOINT`] from a [`CheckpointFloor`] on every request, and resends a
/// request that a replica answers `Unavailable`, re-reading the floor for each attempt.
///
/// Request bodies are buffered so they can be resent. Every `sui.rpc` RPC sends a single request
/// message, so no request is streamed. Each attempt is resent with the request's original
/// `grpc-timeout`, so a configured timeout bounds one attempt, not the call as a whole.
#[derive(Clone)]
pub(crate) struct MonotonicReadLayer {
    floor: CheckpointFloor,
}

/// See [`MonotonicReadLayer`].
#[derive(Clone)]
pub(crate) struct MonotonicRead<S> {
    /// The service below is called once per attempt from inside the response future, so it is
    /// shared rather than borrowed. The client builds a fresh stack for every RPC, so the lock is
    /// never contended.
    inner: Arc<Mutex<S>>,
    floor: CheckpointFloor,
}

impl CheckpointFloor {
    /// Raise the floor to `checkpoint`. A lower value leaves it unchanged.
    pub fn raise(&self, checkpoint: u64) {
        self.0.fetch_max(checkpoint, Ordering::Release);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}

impl MonotonicReadLayer {
    pub(crate) fn new(floor: CheckpointFloor) -> Self {
        Self { floor }
    }
}

impl<S> Layer<S> for MonotonicReadLayer {
    type Service = MonotonicRead<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MonotonicRead {
            inner: Arc::new(Mutex::new(inner)),
            floor: self.floor.clone(),
        }
    }
}

impl<S, ResBody> Service<http::Request<Body>> for MonotonicRead<S>
where
    S: Service<http::Request<Body>, Response = http::Response<ResBody>, Error = BoxError>
        + Send
        + 'static,
    S::Future: Send,
    ResBody: Send + 'static,
{
    type Response = http::Response<ResBody>;
    type Error = BoxError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The inner service's readiness is awaited for every attempt in `call`.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let inner = self.inner.clone();
        let floor = self.floor.clone();

        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = body.collect().await?.to_bytes();

            let mut attempt = 1;
            loop {
                let mut request =
                    http::Request::from_parts(parts.clone(), Body::new(Full::new(body.clone())));
                request
                    .headers_mut()
                    .insert(X_SUI_MIN_CHECKPOINT, HeaderValue::from(floor.get()));

                let response = {
                    let mut inner = inner.lock().await;
                    inner.ready().await?.call(request)
                }
                .await?;

                if attempt == MAX_UNAVAILABLE_ATTEMPTS || !is_unavailable(&response) {
                    return Ok(response);
                }

                attempt += 1;
            }
        })
    }
}

/// Whether the server refused the request with `Unavailable` before sending a response message.
/// A refusal carries its status in the response headers (a trailers-only response), so it can be
/// recognized without reading the body.
fn is_unavailable<B>(response: &http::Response<B>) -> bool {
    response
        .headers()
        .get("grpc-status")
        .is_some_and(|code| tonic::Code::from_bytes(code.as_bytes()) == tonic::Code::Unavailable)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as SyncMutex;

    use tower::service_fn;
    use tower::util::BoxService;

    use super::*;

    type Inner = BoxService<http::Request<Body>, http::Response<Body>, BoxError>;

    /// A client stack whose server answers each call with the next status in `statuses` (`OK`
    /// once they run out) and records the floor each call was stamped with. The floor is raised by
    /// one after every call, as if the caller observed a newer checkpoint between attempts.
    fn scripted(
        floor: CheckpointFloor,
        statuses: Vec<tonic::Code>,
        stamped: Arc<SyncMutex<Vec<String>>>,
    ) -> MonotonicRead<Inner> {
        let floor_raised_by_server = floor.clone();
        let statuses = Arc::new(SyncMutex::new(statuses.into_iter()));
        let server = service_fn(move |request: http::Request<Body>| {
            let header = request.headers()[X_SUI_MIN_CHECKPOINT].to_str().unwrap();
            stamped.lock().unwrap().push(header.to_owned());
            floor_raised_by_server.raise(floor_raised_by_server.get() + 1);

            let status = statuses.lock().unwrap().next();
            async move {
                Ok::<_, BoxError>(match status {
                    Some(code) => tonic::Status::new(code, "scripted").into_http(),
                    None => http::Response::new(Body::empty()),
                })
            }
        });

        MonotonicReadLayer::new(floor).layer(BoxService::new(server))
    }

    async fn send(service: &mut MonotonicRead<Inner>) -> http::Response<Body> {
        service
            .ready()
            .await
            .unwrap()
            .call(http::Request::new(Body::empty()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn unavailable_is_resent_with_the_current_floor() {
        let floor = CheckpointFloor::default();
        floor.raise(10);
        let stamped = Arc::new(SyncMutex::new(vec![]));
        let mut service = scripted(
            floor,
            vec![tonic::Code::Unavailable, tonic::Code::Unavailable],
            stamped.clone(),
        );

        let response = send(&mut service).await;
        assert!(!is_unavailable(&response));
        assert_eq!(*stamped.lock().unwrap(), ["10", "11", "12"]);
    }

    #[tokio::test]
    async fn unavailable_is_returned_after_the_last_attempt() {
        let stamped = Arc::new(SyncMutex::new(vec![]));
        let mut service = scripted(
            CheckpointFloor::default(),
            vec![tonic::Code::Unavailable; MAX_UNAVAILABLE_ATTEMPTS + 1],
            stamped.clone(),
        );

        let response = send(&mut service).await;
        assert!(is_unavailable(&response));
        assert_eq!(stamped.lock().unwrap().len(), MAX_UNAVAILABLE_ATTEMPTS);
    }

    #[tokio::test]
    async fn other_errors_are_not_resent() {
        let stamped = Arc::new(SyncMutex::new(vec![]));
        let mut service = scripted(
            CheckpointFloor::default(),
            vec![tonic::Code::NotFound],
            stamped.clone(),
        );

        let response = send(&mut service).await;
        let status = tonic::Status::from_header_map(response.headers()).unwrap();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert_eq!(stamped.lock().unwrap().len(), 1);
    }
}
