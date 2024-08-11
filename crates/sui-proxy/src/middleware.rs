// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{consumer::ProtobufDecoder, peers::SuiNodeProvider};
use axum::{
    async_trait,
    body::Body,
    body::Bytes,
    extract::{Extension, FromRequest},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::headers::{ContentLength, ContentType};
use axum_extra::typed_header::TypedHeader;
use bytes::Buf;
use hyper::header::CONTENT_ENCODING;
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

static MIDDLEWARE_HEADERS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "middleware_headers",
        "Operations counters and status for axum middleware.",
        &["header", "value"]
    )
    .unwrap()
});

/// we expect sui-node to send us an http header content-length encoding.
pub async fn expect_content_length(
    TypedHeader(content_length): TypedHeader<ContentLength>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    MIDDLEWARE_HEADERS.with_label_values(&["content-length", &format!("{}", content_length.0)]);
    Ok(next.run(request).await)
}

/// we expect sui-node to send us an http header content-type encoding.
pub async fn expect_mysten_proxy_header(
    TypedHeader(content_type): TypedHeader<ContentType>,
    request: Request<Body>,
    next: Next,
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
pub async fn expect_valid_public_key(
    Extension(allower): Extension<Arc<SuiNodeProvider>>,
    Extension(tls_connect_info): Extension<TlsConnectionInfo>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    let Some(public_key) = tls_connect_info.public_key() else {
        error!("unable to obtain public key from connecting client");
        MIDDLEWARE_OPS
            .with_label_values(&["expect_valid_public_key", "missing-public-key"])
            .inc();
        return Err((StatusCode::FORBIDDEN, "unknown clients are not allowed"));
    };
    let Some(peer) = allower.get(public_key) else {
        error!("node with unknown pub key tried to connect {}", public_key);
        MIDDLEWARE_OPS
            .with_label_values(&[
                "expect_valid_public_key",
                "unknown-validator-connection-attempt",
            ])
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
impl<S> FromRequest<S> for LenDelimProtobuf
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let should_be_snappy = req
            .headers()
            .get(CONTENT_ENCODING)
            .map(|v| v.as_bytes() == b"snappy")
            .unwrap_or(false);

        let body = Bytes::from_request(req, state).await.map_err(|e| {
            let msg = format!("error extracting bytes; {e}");
            error!(msg);
            MIDDLEWARE_OPS
                .with_label_values(&["LenDelimProtobuf_from_request", "unable-to-extract-bytes"])
                .inc();
            (e.status(), msg)
        })?;

        let intermediate = if should_be_snappy {
            let mut s = snap::raw::Decoder::new();
            let decompressed = s.decompress_vec(&body).map_err(|e| {
                let msg = format!("unable to decode snappy encoded protobufs; {e}");
                error!(msg);
                MIDDLEWARE_OPS
                    .with_label_values(&[
                        "LenDelimProtobuf_decompress_vec",
                        "unable-to-decode-snappy",
                    ])
                    .inc();
                (StatusCode::BAD_REQUEST, msg)
            })?;
            Bytes::from(decompressed).reader()
        } else {
            body.reader()
        };

        let mut decoder = ProtobufDecoder::new(intermediate);
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
