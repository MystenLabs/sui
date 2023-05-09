// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{consumer::ProtobufDecoder, peers::SuiNodeProvider};
use axum::{
    async_trait,
    body::Bytes,
    extract::{Extension, FromRequest},
    headers::ContentType,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    BoxError, TypedHeader,
};
use bytes::Buf;
use once_cell::sync::Lazy;
use prometheus::{proto::MetricFamily, register_counter_vec, CounterVec};
use std::sync::Arc;
use sui_tls::TlsConnectionInfo;
use tracing::error;

static MIDDLEWARE_OPS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "middleware_operations",
        "Operations counters and status for axum middleware.",
        &["operation", "status"]
    )
    .unwrap()
});

/// we expect sui-node to send us an http header content-type encoding.
pub async fn expect_mysten_proxy_header<B>(
    TypedHeader(content_type): TypedHeader<ContentType>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, (StatusCode, &'static str)> {
    match format!("{content_type}").as_str() {
        prometheus::PROTOBUF_FORMAT => Ok(next.run(request).await),
        ct => {
            error!("invalid content-type; {ct}");
            MIDDLEWARE_OPS
                .with_label_values(&["expect_mysten_proxy_header", "invalid-content-type"])
                .inc();
            Err((StatusCode::BAD_REQUEST, "invalid content-type header"))
        }
    }
}

/// we expect that calling sui-nodes are known on the blockchain and we enforce
/// their pub key tls creds here
pub async fn expect_valid_public_key<B>(
    Extension(allower): Extension<Arc<SuiNodeProvider>>,
    Extension(tls_connect_info): Extension<TlsConnectionInfo>,
    mut request: Request<B>,
    next: Next<B>,
) -> Result<Response, (StatusCode, &'static str)> {
    let Some(peer) = allower.get(tls_connect_info.public_key().unwrap()) else {
        error!("node with unknown pub key tried to connect");
        MIDDLEWARE_OPS
        .with_label_values(&["expect_valid_public_key", "unknown-validator-connection-attempt"])
        .inc();
        return Err((StatusCode::FORBIDDEN, "unknown clients are not allowed"));
    };
    request.extensions_mut().insert(peer);
    Ok(next.run(request).await)
}

// extractor that shows how to consume the request body upfront
#[derive(Debug)]
pub struct LenDelimProtobuf(pub Vec<MetricFamily>);

#[async_trait]
impl<S, B> FromRequest<S, B> for LenDelimProtobuf
where
    S: Send + Sync,
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Rejection = (StatusCode, String);

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        let body = Bytes::from_request(req, state).await.map_err(|e| {
            let msg = format!("error extracting bytes; {e}");
            error!(msg);
            MIDDLEWARE_OPS
                .with_label_values(&["LenDelimProtobuf_from_request", "unable-to-extract-bytes"])
                .inc();
            (StatusCode::BAD_REQUEST, msg)
        })?;
        let mut decoder = ProtobufDecoder::new(body.reader());
        let decoded = decoder.parse::<MetricFamily>().map_err(|e| {
            let msg = format!("unable to decode len deliminated protobufs; {e}");
            error!(msg);
            MIDDLEWARE_OPS
                .with_label_values(&[
                    "LenDelimProtobuf_from_request",
                    "unable-to-decode-protobufs",
                ])
                .inc();
            (StatusCode::BAD_REQUEST, msg)
        })?;
        Ok(Self(decoded))
    }
}
