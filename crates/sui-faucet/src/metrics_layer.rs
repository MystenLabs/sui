// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::Future;
use prometheus::Registry;
use tower::{BoxError, Layer, Service, ServiceExt};
use tracing::info;

use crate::metrics::RequestMetrics;

/// Tower Layer for tracking metrics in Prometheus related to number, success-rate and latency of
/// requests running through service.
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

impl<Inner, Req> Service<Req> for RequestMetricsService<Inner>
where
    Inner: Service<Req, Error = BoxError> + Clone + Send + 'static,
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
        let metrics = self.metrics.clone();
        let inner = self.inner.clone();

        let future = Box::pin(async move {
            metrics.total_requests_received.inc();
            metrics.current_requests_in_flight.inc();

            let timer = metrics.process_latency.start_timer();
            let resp = inner.oneshot(req).await;

            let elapsed = timer.stop_and_record();
            metrics.current_requests_in_flight.dec();

            match &resp {
                Result::Ok(_) => {
                    metrics.total_requests_succeeded.inc();
                    info!("Request succeeded in {:.2}s", elapsed);
                }

                Result::Err(err) => {
                    if err.is::<tower::load_shed::error::Overloaded>() {
                        metrics.total_requests_shed.inc();
                        info!("Request shed in {:.2}s", elapsed);
                    } else {
                        metrics.total_requests_failed.inc();
                        info!("Request failed in {:.2}s", elapsed);
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
