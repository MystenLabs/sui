// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This is a module that records Tokio-tracing [span](https://docs.rs/tracing/latest/tracing/span/index.html)
//! latencies into Prometheus histograms directly.
//! The name of the Prometheus histogram is "tracing_span_latencies[_sum/count/bucket]"
//!
//! There is also the tracing-timing crate, from which this differs significantly:
//! - tracing-timing records latencies between events (logs).  We just want to record the latencies of spans.
//! - tracing-timing does not output to Prometheus, and extracting data from its histograms takes extra CPU
//! - tracing-timing records latencies using HDRHistogram, which is great, but uses extra memory when one
//!   is already using Prometheus
//!
//! Thus this is a much smaller and more focused module.
//!
//! ## Making spans visible
//! This module can only record latencies for spans that get created.  By default, this is controlled by
//! env_filter and logging levels.

use std::time::Instant;

use prometheus::{exponential_buckets, register_histogram_vec_with_registry, Registry};
use tracing::{span, Subscriber};

/// A tokio_tracing Layer that records span latencies into Prometheus histograms
pub struct PrometheusSpanLatencyLayer {
    span_latencies: prometheus::HistogramVec,
}

#[derive(Debug)]
pub enum PrometheusSpanError {
    /// num_buckets must be positive >= 1
    ZeroOrNegativeNumBuckets,
    PromError(prometheus::Error),
}

impl From<prometheus::Error> for PrometheusSpanError {
    fn from(err: prometheus::Error) -> Self {
        Self::PromError(err)
    }
}

const TOP_LATENCY_IN_NS: f64 = 300.0 * 1.0e9;
const LOWEST_LATENCY_IN_NS: f64 = 500.0;

impl PrometheusSpanLatencyLayer {
    /// Create a new layer, injecting latencies into the given registry.
    /// The num_buckets controls how many buckets thus how much memory and time series one
    /// uses up in Prometheus (and in the application).  10 is probably a minimum.
    pub fn try_new(registry: &Registry, num_buckets: usize) -> Result<Self, PrometheusSpanError> {
        if num_buckets < 1 {
            return Err(PrometheusSpanError::ZeroOrNegativeNumBuckets);
        }

        // Histogram for span latencies must accommodate a wide range of possible latencies, so
        // don't use the default Prometheus buckets.  Latencies in NS.  Calculate the multiplier
        // to go from LOWEST to TOP in num_bucket steps, step n+1 = step n * factor.
        let factor = (TOP_LATENCY_IN_NS / LOWEST_LATENCY_IN_NS).powf(1.0 / (num_buckets as f64));
        let buckets = exponential_buckets(LOWEST_LATENCY_IN_NS, factor, num_buckets)?;
        let span_latencies = register_histogram_vec_with_registry!(
            "tracing_span_latencies",
            "Latencies from tokio-tracing spans",
            &["span_name"],
            buckets,
            registry
        )?;
        Ok(Self { span_latencies })
    }
}

struct PromSpanTimestamp(Instant);

impl<S> tracing_subscriber::Layer<S> for PrometheusSpanLatencyLayer
where
    S: Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    fn on_new_span(
        &self,
        _attrs: &span::Attributes,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<S>,
    ) {
        let span = ctx.span(id).unwrap();
        // NOTE: there are other extensions that insert timings.  For example,
        // tracing_subscriber's with_span_events() inserts events at open and close that contain timings.
        // However, we cannot be guaranteed that those events would be turned on.
        span.extensions_mut()
            .insert(PromSpanTimestamp(Instant::now()));
    }

    fn on_close(&self, id: span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(&id).unwrap();
        let start_time = span
            .extensions()
            .get::<PromSpanTimestamp>()
            .expect("Could not find saved timestamp on span")
            .0;
        let elapsed_ns = start_time.elapsed().as_nanos() as u64;
        self.span_latencies
            .with_label_values(&[span.name()])
            .observe(elapsed_ns as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prom_span_latency_init() {
        let registry = prometheus::Registry::new();

        let res = PrometheusSpanLatencyLayer::try_new(&registry, 0);
        assert!(matches!(
            res,
            Err(PrometheusSpanError::ZeroOrNegativeNumBuckets)
        ));
    }
}
