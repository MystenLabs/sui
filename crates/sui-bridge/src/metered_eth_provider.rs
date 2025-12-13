// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{metrics::BridgeMetrics, utils::EthProvider};
use alloy::{
    providers::RootProvider,
    rpc::{
        client::RpcClient,
        json_rpc::{RequestPacket, ResponsePacket},
    },
    transports::{
        Transport, TransportError,
        http::{Http, reqwest},
    },
};
use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tower::Service;
use url::{ParseError, Url};

#[derive(Debug, Clone)]
pub struct MeteredHttpService<S> {
    inner: S,
    metrics: Arc<BridgeMetrics>,
}

impl<S> MeteredHttpService<S> {
    pub fn new(inner: S, metrics: Arc<BridgeMetrics>) -> Self {
        Self { inner, metrics }
    }
}

impl<S> Service<RequestPacket> for MeteredHttpService<S>
where
    S: Transport + Clone,
    S::Future: Send + 'static,
{
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let method_name = match &req {
            RequestPacket::Single(req) => req.method().to_string(),
            RequestPacket::Batch(_) => "batch".to_string(),
        };

        self.metrics
            .eth_rpc_queries
            .with_label_values(&[&method_name])
            .inc();

        let timer = self
            .metrics
            .eth_rpc_queries_latency
            .with_label_values(&[&method_name])
            .start_timer();

        let future = self.inner.call(req);

        // Wrap the future to ensure the timer is dropped when the future completes
        Box::pin(async move {
            let result = future.await;
            // Dropping the timer records the duration in the histogram
            drop(timer);
            result
        })
    }
}

pub fn new_metered_eth_provider(
    url: &str,
    metrics: Arc<BridgeMetrics>,
) -> Result<EthProvider, ParseError> {
    let url: Url = url.parse()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create reqwest client");
    let http_transport = Http::with_client(client, url);
    let metered_transport = MeteredHttpService::new(http_transport, metrics);
    let rpc_client =
        RpcClient::new(metered_transport, false).with_poll_interval(Duration::from_millis(2000));
    Ok(Arc::new(RootProvider::new(rpc_client)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::providers::Provider;
    use prometheus::Registry;

    #[tokio::test]
    async fn test_metered_eth_provider() {
        let metrics = Arc::new(BridgeMetrics::new(&Registry::new()));
        let provider = new_metered_eth_provider("http://localhost:9876", metrics.clone()).unwrap();

        assert_eq!(
            metrics
                .eth_rpc_queries
                .get_metric_with_label_values(&["eth_blockNumber"])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            metrics
                .eth_rpc_queries_latency
                .get_metric_with_label_values(&["eth_blockNumber"])
                .unwrap()
                .get_sample_count(),
            0
        );

        provider.get_block_number().await.unwrap_err(); // the rpc cal will fail but we don't care

        assert_eq!(
            metrics
                .eth_rpc_queries
                .get_metric_with_label_values(&["eth_blockNumber"])
                .unwrap()
                .get(),
            1
        );
        assert_eq!(
            metrics
                .eth_rpc_queries_latency
                .get_metric_with_label_values(&["eth_blockNumber"])
                .unwrap()
                .get_sample_count(),
            1
        );
    }
}
