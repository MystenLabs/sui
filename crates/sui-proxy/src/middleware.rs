// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::peers::SuiNodeProvider;
use axum::{
    extract::Extension,
    headers::ContentType,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    TypedHeader,
};
use std::sync::Arc;
use sui_tls::TlsConnectionInfo;
use tracing::error;

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
        return Err((StatusCode::FORBIDDEN, "unknown clients are not allowed"));
    };

    request.extensions_mut().insert(peer);
    Ok(next.run(request).await)
}
