// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use hyper::service::Service;
use hyper::{body, http, Body, Request, Response};
use jsonrpsee::core::__reexports::serde_json;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry, HistogramVec,
    IntCounterVec,
};
use serde::Deserialize;
use tokio::time::Instant;
use tower::Layer;

#[derive(Debug, Clone)]
pub struct MetricsLayer {
    metrics: Arc<Metrics>,
}
impl MetricsLayer {
    pub fn new(registry: &prometheus::Registry) -> Self {
        let metrics = Arc::new(Metrics {
            requests_by_route: register_int_counter_vec_with_registry!(
                "rpc_requests_by_route",
                "Number of requests by route",
                &["route"],
                registry,
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec_with_registry!(
                "req_latency_by_route",
                "Latency of a request by route",
                &["route"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            errors_by_route: register_int_counter_vec_with_registry!(
                "errors_by_route",
                "Number of errors by route",
                &["route"],
                registry,
            )
            .unwrap(),
        });

        Self { metrics }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = JsonRpcMetricService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        JsonRpcMetricService::new(inner, self.metrics.clone())
    }
}

#[derive(Debug, Clone)]
pub struct JsonRpcMetricService<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

#[derive(Debug, Clone)]
pub struct Metrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
    /// Failed requests by route
    errors_by_route: IntCounterVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl<S> JsonRpcMetricService<S> {
    pub fn new(inner: S, metrics: Arc<Metrics>) -> Self {
        Self { inner, metrics }
    }
}

impl<S> Service<Request<Body>> for JsonRpcMetricService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Response: 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = Box<dyn Error + Send + Sync + 'static>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let started_at = Instant::now();
        let metrics = self.metrics.clone();
        let mut inner = self.inner.clone();

        let res_fut = async move {
            // Parse request to retrieve RPC method name.
            let (rpc_name, req) = if is_json(req.headers().get(http::header::CONTENT_TYPE)) {
                let (part, body) = req.into_parts();
                let bytes = body::to_bytes(body).await?;
                #[derive(Deserialize)]
                struct RPCRequest {
                    method: String,
                }

                let name = serde_json::from_slice::<RPCRequest>(&bytes)
                    .ok()
                    .map(|rpc| rpc.method);

                (name, Request::from_parts(part, Body::from(bytes)))
            } else {
                (None, req)
            };

            let fut = inner.call(req);
            let res = fut.await.map_err(|err| err.into())?;

            // Record metrics if the request is a http RPC request.
            if let Some(name) = rpc_name {
                metrics.requests_by_route.with_label_values(&[&name]).inc();
                let req_latency_secs = (Instant::now() - started_at).as_secs_f64();
                metrics
                    .req_latency_by_route
                    .with_label_values(&[&name])
                    .observe(req_latency_secs);

                if !res.status().is_server_error() {
                    metrics.errors_by_route.with_label_values(&[&name]).inc();
                }
            }
            Ok(res)
        };
        Box::pin(res_fut)
    }
}

fn is_json(content_type: Option<&hyper::header::HeaderValue>) -> bool {
    content_type
        .and_then(|val| val.to_str().ok())
        .map_or(false, |content| {
            content.eq_ignore_ascii_case("application/json")
                || content.eq_ignore_ascii_case("application/json; charset=utf-8")
                || content.eq_ignore_ascii_case("application/json;charset=utf-8")
        })
}
