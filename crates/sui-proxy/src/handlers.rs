// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::admin::ReqwestClient;
use crate::consumer::{convert_to_remote_write, NodeMetric};
use crate::middleware::LenDelimProtobuf;
use crate::peers::SuiPeer;
use axum::{
    extract::{ConnectInfo, Extension},
    http::StatusCode,
};
use multiaddr::Multiaddr;
use std::net::SocketAddr;

/// Publish handler which receives metrics from nodes.  Nodes will call us at this endpoint
/// and we relay them to the upstream tsdb
///
/// Clients will receive a response after successfully relaying the metrics upstream
pub async fn publish_metrics(
    Extension(network): Extension<String>,
    Extension(client): Extension<ReqwestClient>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(peer): Extension<SuiPeer>,
    LenDelimProtobuf(data): LenDelimProtobuf,
) -> (StatusCode, &'static str) {
    convert_to_remote_write(
        client.clone(),
        NodeMetric {
            host: peer.name,
            network,
            data,
            peer_addr: Multiaddr::from(addr.ip()),
            public_key: peer.public_key,
        },
    )
    .await
}
