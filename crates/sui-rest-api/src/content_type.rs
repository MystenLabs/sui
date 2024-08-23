// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::{self, header, HeaderMap, StatusCode};
use mime::Mime;
use tap::Pipe;

/// `Content-Type` header, defined in [RFC7231](http://tools.ietf.org/html/rfc7231#section-3.1.1.5)
#[derive(Debug, Clone)]
pub struct ContentType(pub Mime);

impl ContentType {
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        parse_content_type(headers).ok()?.map(Self)
    }
}

fn parse_content_type(headers: &HeaderMap) -> Result<Option<Mime>, mime::FromStrError> {
    let Some(header) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|hval| hval.to_str().ok())
    else {
        return Ok(None);
    };

    let mime: Mime = header.parse()?;
    Ok(Some(mime))
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for ContentType
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        parse_content_type(&parts.headers)
            .map_err(|_| (StatusCode::BAD_REQUEST, "invalid Content-Type mime"))?
            .ok_or((StatusCode::BAD_REQUEST, "Content-Type header missing"))?
            .pipe(Self)
            .pipe(Ok)
    }
}
