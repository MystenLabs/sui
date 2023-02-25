// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::body::Bytes;
use prometheus_parse::Scrape;
use tokio::sync::mpsc::Receiver;
use tracing::error;
/// NodeMetric is a placeholder for metric data
/// tbd in future PR
#[derive(Debug)]
pub struct NodeMetric {
    pub host: String,
    pub data: Bytes, // raw post data from node
}

/// An UpstreamConsumer accepts bytes from calling clients and is
/// responsible for sending them on to upstream services to store
/// the relayed metric
pub struct UpstreamConsumer {
    pub network: String,
    pub receiver: Receiver<NodeMetric>,
}

impl UpstreamConsumer {
    pub fn new(network: String, receiver: Receiver<NodeMetric>) -> Self {
        Self { network, receiver }
    }
    pub async fn run(&mut self) {
        while let Some(nm) = self.receiver.recv().await {
            let network = self.network.to_owned();
            tokio::spawn(async move {
                let Ok(data) = std::str::from_utf8(&nm.data) else {
                    error!("unable to decode bytes from relayed metrics for host: {}", nm.host);
                    return;
                };
                let lines = data.lines().map(|s| Ok(s.to_owned()));
                let Ok(mut metrics) = Scrape::parse(lines) else {
                    error!("unable to parse exposition data for host: {}", nm.host);
                    return;
                };

                for s in metrics.samples.iter_mut() {
                    s.labels.insert("host".to_string(), nm.host.to_owned());
                    s.labels.insert("network".to_string(), network.to_owned());
                }
            });
        }
    }
}
