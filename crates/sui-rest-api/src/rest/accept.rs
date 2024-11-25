// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::{self, header, HeaderMap};
use mime::Mime;

// TODO look into utilizing the following way to signal the expected types since bcs doesn't
// include type information
// "application/x.sui.<type>+bcs"
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_PROTOBUF: &str = "application/x-protobuf";

/// `Accept` header, defined in [RFC7231](http://tools.ietf.org/html/rfc7231#section-5.3.2)
#[derive(Debug, Clone)]
pub struct Accept(pub Vec<Mime>);

fn parse_accept(headers: &HeaderMap) -> Vec<Mime> {
    let mut items = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|hval| hval.to_str().ok())
        .flat_map(|s| s.split(',').map(str::trim))
        .filter_map(|item| {
            let mime: Mime = item.parse().ok()?;
            let q = mime
                .get_param("q")
                .and_then(|value| Some((value.as_str().parse::<f32>().ok()? * 1000.0) as i32))
                .unwrap_or(1000);
            Some((mime, q))
        })
        .collect::<Vec<_>>();
    items.sort_by(|(_, qa), (_, qb)| qb.cmp(qa));
    items.into_iter().map(|(mime, _)| mime).collect()
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for Accept
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(parse_accept(&parts.headers)))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AcceptFormat {
    Json,
    Bcs,
    // Protobuf,
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for AcceptFormat
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        s: &S,
    ) -> Result<Self, Self::Rejection> {
        let accept = Accept::from_request_parts(parts, s).await?;

        for mime in accept.0 {
            let essence = mime.essence_str();

            if essence == mime::APPLICATION_JSON.essence_str() {
                return Ok(Self::Json);
            } else if essence == APPLICATION_BCS {
                return Ok(Self::Bcs);
            }
        }

        Ok(Self::Json)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AcceptJsonProtobufBcs {
    Json,
    Protobuf,
    Bcs,
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for AcceptJsonProtobufBcs
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        s: &S,
    ) -> Result<Self, Self::Rejection> {
        let accept = Accept::from_request_parts(parts, s).await?;

        for mime in accept.0 {
            let essence = mime.essence_str();

            if essence == mime::APPLICATION_JSON.essence_str() {
                return Ok(Self::Json);
            } else if essence == APPLICATION_PROTOBUF {
                return Ok(Self::Protobuf);
            } else if essence == APPLICATION_BCS {
                return Ok(Self::Bcs);
            }
        }

        Ok(Self::Json)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use axum::{extract::FromRequest, http::Request};
    use http::header;

    use super::*;

    #[tokio::test]
    async fn test_accept() {
        let req = Request::builder()
            .header(
                header::ACCEPT,
                "text/html, text/yaml;q=0.5, application/xhtml+xml, application/xml;q=0.9, */*;q=0.1",
            )
            .body(axum::body::Body::empty())
            .unwrap();
        let accept = Accept::from_request(req, &()).await.unwrap();
        assert_eq!(
            accept.0,
            &[
                Mime::from_str("text/html").unwrap(),
                Mime::from_str("application/xhtml+xml").unwrap(),
                Mime::from_str("application/xml;q=0.9").unwrap(),
                Mime::from_str("text/yaml;q=0.5").unwrap(),
                Mime::from_str("*/*;q=0.1").unwrap()
            ]
        );
    }

    #[tokio::test]
    async fn test_accept_format() {
        let req = Request::builder()
            .header(header::ACCEPT, "*/*, application/bcs")
            .body(axum::body::Body::empty())
            .unwrap();
        let accept = AcceptFormat::from_request(req, &()).await.unwrap();
        assert_eq!(accept, AcceptFormat::Bcs);

        let req = Request::builder()
            .header(header::ACCEPT, "*/*")
            .body(axum::body::Body::empty())
            .unwrap();
        let accept = AcceptFormat::from_request(req, &()).await.unwrap();
        assert_eq!(accept, AcceptFormat::Json);

        let req = Request::builder()
            .header(header::ACCEPT, "application/json, application/bcs")
            .body(axum::body::Body::empty())
            .unwrap();
        let accept = AcceptFormat::from_request(req, &()).await.unwrap();
        assert_eq!(accept, AcceptFormat::Json);

        let req = Request::builder()
            .header(header::ACCEPT, "application/bcs, application/json")
            .body(axum::body::Body::empty())
            .unwrap();
        let accept = AcceptFormat::from_request(req, &()).await.unwrap();
        assert_eq!(accept, AcceptFormat::Bcs);
    }
}
