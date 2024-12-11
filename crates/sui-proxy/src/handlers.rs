// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::admin::{Labels, ReqwestClient};
use crate::consumer::{convert_to_remote_write, populate_labels, NodeMetric};
use crate::histogram_relay::HistogramRelay;
use crate::middleware::LenDelimProtobuf;
use crate::peers::AllowedPeer;
use axum::{
    extract::{ConnectInfo, Extension},
    http::StatusCode,
};
use multiaddr::Multiaddr;
use once_cell::sync::Lazy;
use prometheus::{register_counter_vec, register_histogram_vec};
use prometheus::{CounterVec, HistogramVec};
use std::net::SocketAddr;

static HANDLER_HITS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "http_handler_hits",
        "Number of HTTP requests made.",
        &["handler", "remote"]
    )
    .unwrap()
});

static HTTP_HANDLER_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "http_handler_duration_seconds",
        "The HTTP request latencies in seconds.",
        &["handler", "remote"],
        vec![
            1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5, 3.75, 4.0, 4.25, 4.5, 4.75,
            5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0
        ],
    )
    .unwrap()
});

/// Publish handler which receives metrics from nodes.  Nodes will call us at this endpoint
/// and we relay them to the upstream tsdb
///
/// Clients will receive a response after successfully relaying the metrics upstream
pub async fn publish_metrics(
    Extension(labels): Extension<Labels>,
    Extension(client): Extension<ReqwestClient>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(AllowedPeer { name, public_key }): Extension<AllowedPeer>,
    Extension(relay): Extension<HistogramRelay>,
    LenDelimProtobuf(data): LenDelimProtobuf,
) -> (StatusCode, &'static str) {
    HANDLER_HITS
        .with_label_values(&["publish_metrics", &name])
        .inc();
    let timer = HTTP_HANDLER_DURATION
        .with_label_values(&["publish_metrics", &name])
        .start_timer();
    let data = populate_labels(name, labels.network, labels.inventory_hostname, data);
    relay.submit(data.clone());
    let response = convert_to_remote_write(
        client.clone(),
        NodeMetric {
            data,
            peer_addr: Multiaddr::from(addr.ip()),
            public_key,
        },
    )
    .await;
    timer.observe_duration();
    response
}
