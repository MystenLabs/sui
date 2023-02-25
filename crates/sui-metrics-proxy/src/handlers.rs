// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::error;

use crate::channels::NodeMetric;

/// Publish handler which receives metrics from nodes.  Nodes will call us at this endpoint
/// and we relay them to the upstream tsdb
///
/// An mpsc is used within this handler so that we can immediately return an accept to calling nodes.
/// Downstream processing failures may still result in metrics being dropped.
pub async fn publish_metrics(
    State(state): State<Arc<Sender<NodeMetric>>>,
    headers: HeaderMap,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Some(reason) = validate_headers(&headers) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONNECTION, "close")],
            reason,
        );
    }
    let host = match headers.get(header::HOST).map(|x| x.to_string()) {
        Some(host) => host,
        None => "unknown".to_string(),
    };

    let data = match hyper::body::to_bytes(request.into_body()).await {
        Ok(data) => data,
        Err(_e) => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONNECTION, "close")],
                "unable to extract post body",
            );
        }
    };

    let sender = state.clone();
    if let Err(e) = sender.send(NodeMetric { host, data }).await {
        error!("unable to queue; unable to send to consumer; {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONNECTION, "close")],
            "unable to queue metrics",
        );
    }
    (StatusCode::OK, [(header::CONNECTION, "close")], "accepted")
}

fn validate_headers<'a>(headers: &HeaderMap) -> Option<&'a str> {
    match headers.get(header::CONTENT_TYPE).map(|v| v.as_bytes()) {
        Some(b"application/mysten.proxy.promexposition") => None,
        _ => {
            let v: &'a str = "bad content-type header";
            Some(v)
        }
    }
}

/// Additional conversion methods for `HeaderValue`.
pub trait HeaderValueExt {
    fn to_string(&self) -> String;
}

impl HeaderValueExt for HeaderValue {
    fn to_string(&self) -> String {
        self.to_str().unwrap_or_default().to_string()
    }
}
