// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cursor::{transform_json_body, update_pagination_cursor_state, PaginationCursorState};
use crate::metrics::AppMetrics;
use axum::http::HeaderValue;
use axum::{
    body::Body,
    extract::{Request, State},
    http::request::Parts,
    http::StatusCode,
    response::Response,
};
use bytes::Bytes;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};
use url::Url;

#[derive(Clone)]
pub struct AppState {
    fullnode_address: Url,
    unsupported_methods: HashSet<String>,
    allowed_origins: Option<HashSet<String>>,
    cursor_state: Arc<PaginationCursorState>,
    metrics: AppMetrics,
}

impl AppState {
    pub fn new(
        fullnode_address: Url,
        unsupported_methods: HashSet<String>,
        allowed_origins: Option<HashSet<String>>,
        cursor_state: Arc<PaginationCursorState>,
        metrics: AppMetrics,
    ) -> Self {
        info!(
            "Creating app state with allowed origins: {:?} and unsupported methods: {:?}",
            allowed_origins, unsupported_methods
        );
        Self {
            fullnode_address,
            unsupported_methods,
            allowed_origins,
            cursor_state,
            metrics,
        }
    }
}

pub async fn proxy_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    let (mut parts, body) = request.into_parts();

    let mut body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to read request body"))
                .unwrap());
        }
    };

    debug!(
        "Request method: {:?}, headers: {:?}",
        parts.method, parts.headers
    );

    if let Some(allowed_origins) = &state.allowed_origins {
        match parts.headers.get("origin") {
            Some(origin) => {
                if !allowed_origins.contains(origin.to_str().unwrap()) {
                    debug!("Dropping request from origin: {}", origin.to_str().unwrap());
                    return Ok(Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .body(Body::from("Forbidden"))
                        .unwrap());
                }
            }
            None => {
                debug!("Dropping request with no origin header");
                return Ok(Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::from("Forbidden"))
                    .unwrap());
            }
        }
    }

    match parts
        .headers
        .get("Client-Request-Method")
        .and_then(|h| h.to_str().ok())
    {
        Some(method) if state.unsupported_methods.contains(method) => {
            debug!("Dropping {method} request");
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(format!("Dropped {method} request")))
                .unwrap())
        }
        _ => {
            let mut json_body = match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(json_body) => json_body,
                Err(_) => {
                    debug!("Failed to parse request body as JSON");
                    return Ok(proxy_request(&state, parts, body_bytes).await?.0);
                }
            };
            let method = json_body
                .get("method")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());
            let params = json_body
                .get("params")
                .and_then(|p| p.as_array())
                .map(|a| a.to_vec())
                .unwrap_or_default();
            match method {
                Some(method) if state.unsupported_methods.contains(&method) => {
                    info!("Dropping {method} request with params: {params:?}");
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .body(Body::from(format!("Dropped {method} request")))
                        .unwrap())
                }
                Some(method) => {
                    debug!("Transforming {method} request with params: {params:?}");
                    match transform_json_body(&mut json_body, &method, &params, &state.cursor_state)
                    {
                        Ok(true) => {
                            debug!("Transformed json_body: {json_body:?}");
                            body_bytes = match serde_json::to_vec(&json_body) {
                                Ok(bytes) => Bytes::from(bytes),
                                Err(_) => {
                                    return Ok(Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(Body::from("Failed to serialize transformed JSON"))
                                        .unwrap());
                                }
                            };
                            // Now that the content has changed, update content length header if it exists
                            if parts.headers.get("Content-Length").is_some() {
                                parts.headers.insert(
                                    "Content-Length",
                                    HeaderValue::from_str(&body_bytes.len().to_string()).unwrap(),
                                );
                            }
                        }
                        Ok(false) => {
                            // Do nothing, no cursor transformation done so no need to update body bytes or content length header.
                        }
                        Err(_) => {
                            debug!("Failed to transform json_body: {json_body:?}");
                            return Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Failed to transform body json"))
                                .unwrap());
                        }
                    }
                    let (response, response_bytes) =
                        proxy_request(&state, parts, body_bytes).await?;
                    if response.status().is_success() {
                        let res = update_pagination_cursor_state(
                            &response_bytes,
                            &method,
                            &params,
                            &state.cursor_state,
                        );
                        if res.is_err() {
                            warn!(
                                "Failed to update pagination cursor state: {}",
                                res.err().unwrap()
                            );
                        }
                    }
                    Ok(response)
                }
                _ => {
                    // We can't find out what the method is so we directly proxy the request.
                    Ok(proxy_request(&state, parts, body_bytes).await?.0)
                }
            }
        }
    }
}

async fn proxy_request(
    state: &AppState,
    parts: Parts,
    body_bytes: Bytes,
) -> Result<(Response, bytes::Bytes), (StatusCode, String)> {
    info!(
        "Proxying request: method={:?}, uri={:?}, headers={:?}, body_len={}",
        parts.method,
        parts.uri,
        parts.headers,
        body_bytes.len(),
    );

    let metrics = &state.metrics;
    let method_str = parts.method.as_str();

    let timer_histogram = metrics.request_latency.with_label_values(&[method_str]);
    let _timer = timer_histogram.start_timer();

    metrics
        .request_size_bytes
        .with_label_values(&[method_str])
        .observe(body_bytes.len() as f64);

    let mut target_url = state.fullnode_address.clone();

    if let Some(query) = parts.uri.query() {
        target_url.set_query(Some(query));
    }

    // remove host header to avoid interfering with reqwest auto-host header
    let mut headers = parts.headers.clone();
    headers.remove("host");

    let request_builder = reqwest::ClientBuilder::new()
        .http2_prior_knowledge()
        .build()
        .expect("Failed to build HTTP/2 client")
        .request(parts.method.clone(), target_url)
        .headers(headers)
        .body(body_bytes.clone());
    info!("Request builder: {:?}", request_builder);

    let upstream_start = Instant::now();
    let response = match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16().to_string();
            metrics
                .upstream_response_latency
                .with_label_values(&[method_str, &status])
                .observe(upstream_start.elapsed().as_secs_f64());
            metrics
                .requests_total
                .with_label_values(&[method_str, &status])
                .inc();
            debug!("Response: {:?}", response);
            response
        }
        Err(e) => {
            warn!("Failed to send request: {}", e);
            metrics
                .upstream_response_latency
                .with_label_values(&[method_str, "error"])
                .observe(upstream_start.elapsed().as_secs_f64());
            metrics
                .requests_total
                .with_label_values(&[method_str, "error"])
                .inc();
            if e.is_timeout() {
                metrics
                    .timeouts_total
                    .with_label_values(&[method_str])
                    .inc();
            }
            return Err((StatusCode::BAD_GATEWAY, format!("Request failed: {}", e)));
        }
    };

    let response_headers = response.headers().clone();
    let response_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Failed to read response body: {}", e);
            metrics
                .error_counts
                .with_label_values(&[method_str, "response_body_read"])
                .inc();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read response body".to_string(),
            ));
        }
    };
    metrics
        .response_size_bytes
        .with_label_values(&[method_str])
        .observe(response_bytes.len() as f64);

    // Debug: Print request method/params and response bytes in readable format
    let request_info =
        if let Ok(request_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            let method = request_json
                .get("method")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            let params = request_json
                .get("params")
                .unwrap_or(&serde_json::Value::Null);
            format!(
                "Method: {}, Params: {}",
                method,
                serde_json::to_string(params).unwrap_or_else(|_| "null".to_string())
            )
        } else {
            "Could not parse request".to_string()
        };

    if let Ok(response_str) = std::str::from_utf8(&response_bytes) {
        let truncated_response = if response_str.len() > 1000 {
            let truncate_at = response_str
                .char_indices()
                .nth(1000)
                .map(|(i, _)| i)
                .unwrap_or(1000);
            format!(
                "{}... (truncated, {} total chars)",
                &response_str[..truncate_at],
                response_str.len()
            )
        } else {
            response_str.to_string()
        };
        info!(
            "Request: {} | Response body (UTF-8): {}",
            request_info, truncated_response
        );
    } else {
        info!(
            "Request: {} | Response body is not valid UTF-8, {} bytes",
            request_info,
            response_bytes.len()
        );
    }

    let mut resp = Response::new(response_bytes.clone().into());
    for (name, value) in response_headers {
        if let Some(name) = name {
            resp.headers_mut().insert(name, value);
        }
    }

    Ok((resp, response_bytes))
}
