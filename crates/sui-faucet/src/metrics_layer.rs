// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::Future;
use http::StatusCode;
use prometheus::{HistogramTimer, Registry};
use tower::{load_shed::error::Overloaded, BoxError, Layer, Service, ServiceExt};
use tracing::{error, info, warn};

use crate::metrics::RequestMetrics;

/// Tower Layer for tracking metrics in Prometheus related to number, success-rate and latency of
/// requests running through service.
#[derive(Clone)]
pub struct RequestMetricsLayer {
    metrics: Arc<RequestMetrics>,
}

#[derive(Clone)]
pub struct RequestMetricsService<Inner> {
    inner: Inner,
    metrics: Arc<RequestMetrics>,
}

pub struct RequestMetricsFuture<Res> {
    future: Pin<Box<dyn Future<Output = Result<Res, BoxError>> + Send>>,
}

struct MetricsGuard {
    timer: Option<HistogramTimer>,
    metrics: Arc<RequestMetrics>,
}

impl RequestMetricsLayer {
    pub fn new(registry: &Registry) -> Self {
        Self {
            metrics: Arc::new(RequestMetrics::new(registry)),
        }
    }
}

impl<Inner> Layer<Inner> for RequestMetricsLayer {
    type Service = RequestMetricsService<Inner>;
    fn layer(&self, inner: Inner) -> Self::Service {
        RequestMetricsService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

impl<Inner, Req, Body> Service<Req> for RequestMetricsService<Inner>
where
    Inner: Service<Req, Response = http::Response<Body>, Error = BoxError> + Clone + Send + 'static,
    Inner::Future: Send,
    Req: Send + 'static,
{
    type Response = Inner::Response;
    type Error = BoxError;
    type Future = RequestMetricsFuture<Self::Response>;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let metrics = MetricsGuard::new(self.metrics.clone());
        let inner = self.inner.clone();

        let future = Box::pin(async move {
            let resp = inner.oneshot(req).await;
            match &resp {
                Result::Ok(resp) if !resp.status().is_success() => {
                    metrics.failed(None, Some(resp.status()))
                }
                Result::Ok(_) => metrics.succeeded(),
                Result::Err(err) => {
                    if err.is::<Overloaded>() {
                        metrics.shed();
                    } else {
                        metrics.failed(Some(err), None);
                    }
                }
            }

            resp
        });

        RequestMetricsFuture { future }
    }
}

impl<Res> Future for RequestMetricsFuture<Res> {
    type Output = Result<Res, BoxError>;
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        Future::poll(self.future.as_mut(), ctx)
    }
}

impl MetricsGuard {
    fn new(metrics: Arc<RequestMetrics>) -> Self {
        metrics.total_requests_received.inc();
        metrics.current_requests_in_flight.inc();
        MetricsGuard {
            timer: Some(metrics.process_latency.start_timer()),
            metrics,
        }
    }

    fn succeeded(mut self) {
        let elapsed = self.timer.take().unwrap().stop_and_record();
        self.metrics.total_requests_succeeded.inc();
        info!("Request succeeded in {:.2}s", elapsed);
    }

    fn failed(mut self, error: Option<&BoxError>, status: Option<StatusCode>) {
        let elapsed = self.timer.take().unwrap().stop_and_record();
        let code = status
            .map(|c| c.as_str().to_string())
            .unwrap_or_else(|| "no_code".to_string());
        self.metrics.total_requests_failed.inc();

        if let Some(err) = error {
            error!(
                "Request failed in {:.2}s, error {:?}, code {}",
                elapsed, err, code
            );
        } else {
            warn!("Request failed in {:.2}s, code: {}", elapsed, code);
        }
    }

    fn shed(mut self) {
        let elapsed = self.timer.take().unwrap().stop_and_record();
        self.metrics.total_requests_shed.inc();
        info!("Request shed in {:.2}s", elapsed);
    }
}

impl Drop for MetricsGuard {
    fn drop(&mut self) {
        self.metrics.current_requests_in_flight.dec();

        // Request was still in flight when the guard was dropped, implying the client disconnected.
        if let Some(timer) = self.timer.take() {
            let elapsed = timer.stop_and_record();
            self.metrics.total_requests_disconnected.inc();
            info!("Request disconnected in {:.2}s", elapsed);
        }
    }
}
