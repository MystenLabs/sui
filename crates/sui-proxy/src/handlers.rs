// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::admin::ReqwestClient;
use crate::consumer::{convert_to_remote_write, NodeMetric};
use crate::peers::SuiPeer;
use axum::{
    body::Body,
    extract::{ConnectInfo, Extension},
    http::{Request, StatusCode},
};
use multiaddr::Multiaddr;
use std::net::SocketAddr;

/// Publish handler which receives metrics from nodes.  Nodes will call us at this endpoint
/// and we relay them to the upstream tsdb
///
/// An mpsc is used within this handler so that we can immediately return an accept to calling nodes.
/// Downstream processing failures may still result in metrics being dropped.
pub async fn publish_metrics(
    Extension(network): Extension<String>,
    Extension(client): Extension<ReqwestClient>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(peer): Extension<SuiPeer>,
    request: Request<Body>,
) -> (StatusCode, &'static str) {
    let data = match hyper::body::to_bytes(request.into_body()).await {
        Ok(data) => data,
        Err(_e) => {
            return (StatusCode::BAD_REQUEST, "unable to extract post body");
        }
    };

    convert_to_remote_write(
        client.clone(),
        NodeMetric {
            name: peer.name,
            network,
            data,
            peer_addr: Multiaddr::from(addr.ip()),
            public_key: peer.public_key,
        },
    )
    .await
}
