// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{CLIENT_TARGET_API_VERSION_HEADER, MAX_REQUEST_SIZE};
use hyper::{http, Body, Method, Request, Response};
use jsonrpsee::core::__reexports::serde_json;
use jsonrpsee::core::error::GenericTransportError;
use jsonrpsee::core::http_helpers::read_body;
use jsonrpsee::types::Request as RpcRequest;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use sui_open_rpc::MethodRouting;
use tower::{Layer, Service};

#[derive(Debug, Clone)]
pub struct RoutingLayer {
    routes: HashMap<String, MethodRouting>,
    disable_routing: bool,
}

impl RoutingLayer {
    pub fn new(routes: HashMap<String, MethodRouting>, disable_routing: bool) -> Self {
        Self {
            routes,
            disable_routing,
        }
    }
}

impl<S> Layer<S> for RoutingLayer {
    type Service = RpcRoutingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RpcRoutingService::new(inner, self.routes.clone(), self.disable_routing)
    }
}

#[derive(Debug, Clone)]
pub struct RpcRoutingService<S> {
    inner: S,
    routes: HashMap<String, MethodRouting>,
    route_to_methods: HashSet<String>,
    disable_routing: bool,
}

impl<S> RpcRoutingService<S> {
    pub fn new(inner: S, routes: HashMap<String, MethodRouting>, disable_routing: bool) -> Self {
        let route_to_methods = routes.values().map(|v| v.route_to.clone()).collect();
        Self {
            inner,
            routes,
            route_to_methods,
            disable_routing,
        }
    }
}

impl<S> Service<Request<Body>> for RpcRoutingService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Response: 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = Box<dyn Error + Send + Sync + 'static>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let clone = self.inner.clone();
        let routes = self.routes.clone();
        let route_to_methods = self.route_to_methods.clone();
        let disable_routing = self.disable_routing;
        // take the service that was ready
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let res_fut = async move {
            // Get version from header.
            let version = req
                .headers()
                .get(CLIENT_TARGET_API_VERSION_HEADER)
                .as_ref()
                .and_then(|h| h.to_str().ok().map(|s| s.to_string()));

            let req = if req.method() == Method::POST && is_json(&req) {
                let (parts, body) = req.into_parts();
                let (body, is_single) =
                    // The body will be consumed if anything goes wrong here, returning error response if failed.
                    match read_body(&parts.headers, body, MAX_REQUEST_SIZE).await {
                        Ok(r) => r,
                        Err(GenericTransportError::TooLarge) => {
                            return Ok(response::too_large(MAX_REQUEST_SIZE))
                        }
                        Err(GenericTransportError::Malformed) => return Ok(response::malformed()),
                        Err(GenericTransportError::Inner(e)) => {
                            tracing::error!("Internal error reading request body: {}", e);
                            return Ok(response::internal_error());
                        }
                    };
                let body = if is_single {
                    process_single_request(
                        &body,
                        &version,
                        &routes,
                        &route_to_methods,
                        disable_routing,
                    )
                } else {
                    process_batched_requests(
                        &body,
                        &version,
                        &routes,
                        &route_to_methods,
                        disable_routing,
                    )
                };
                Request::from_parts(parts, Body::from(body))
            } else {
                req
            };
            inner.call(req).await.map_err(|err| err.into())
        };
        Box::pin(res_fut)
    }
}

fn process_batched_requests(
    body: &[u8],
    version: &Option<String>,
    routes: &HashMap<String, MethodRouting>,
    route_to_methods: &HashSet<String>,
    disable_routing: bool,
) -> Vec<u8> {
    let Ok(requests) = serde_json::from_slice::<Vec<&[u8]>>(body) else{
        return body.to_vec();
    };
    let mut processed_reqs = Vec::new();
    for request in requests {
        let req =
            process_single_request(request, version, routes, route_to_methods, disable_routing);
        processed_reqs.push(req);
    }
    if let Ok(request) = serde_json::to_vec(&processed_reqs) {
        request
    } else {
        body.to_vec()
    }
}

// try to process the rpc request, return the original values if fail to parse the request.
fn process_single_request(
    body: &[u8],
    version: &Option<String>,
    routes: &HashMap<String, MethodRouting>,
    route_to_methods: &HashSet<String>,
    disable_routing: bool,
) -> Vec<u8> {
    let Ok(mut request) = serde_json::from_slice::<RpcRequest>(body) else{
        return body.to_vec();
    };

    let mut modified = false;

    // Reject direct access to the old methods
    if route_to_methods.contains(request.method.as_ref()) {
        request.method = "INVALID_ROUTING".into();
        modified = true;
    } else {
        // Modify the method name if routing is enabled
        if !disable_routing {
            if let Some(version) = version {
                if let Some(route) = routes.get(request.method.as_ref()) {
                    if route.matches(version) {
                        request.method = route.route_to.clone().into();
                        modified = true;
                    }
                };
            }
        }
    }

    if !modified {
        return body.to_vec();
    }

    if let Ok(result) = serde_json::to_vec(&request) {
        result
    } else {
        body.to_vec()
    }
}

// error responses borrowed from jsonrpsee
mod response {
    use jsonrpsee::core::__reexports::serde_json;
    use jsonrpsee::types::error::{reject_too_big_request, ErrorCode};
    use jsonrpsee::types::{ErrorResponse, Id};
    const JSON: &str = "application/json; charset=utf-8";

    pub(crate) fn too_large(limit: u32) -> hyper::Response<hyper::Body> {
        let error = serde_json::to_string(&ErrorResponse::borrowed(
            reject_too_big_request(limit),
            Id::Null,
        ))
        .expect("built from known-good data; qed");
        from_template(hyper::StatusCode::PAYLOAD_TOO_LARGE, error, JSON)
    }

    pub(crate) fn internal_error() -> hyper::Response<hyper::Body> {
        let error = serde_json::to_string(&ErrorResponse::borrowed(
            ErrorCode::InternalError.into(),
            Id::Null,
        ))
        .expect("built from known-good data; qed");

        from_template(hyper::StatusCode::INTERNAL_SERVER_ERROR, error, JSON)
    }

    pub(crate) fn malformed() -> hyper::Response<hyper::Body> {
        let error = serde_json::to_string(&ErrorResponse::borrowed(
            ErrorCode::ParseError.into(),
            Id::Null,
        ))
        .expect("built from known-good data; qed");

        from_template(hyper::StatusCode::BAD_REQUEST, error, JSON)
    }

    fn from_template<S: Into<hyper::Body>>(
        status: hyper::StatusCode,
        body: S,
        content_type: &'static str,
    ) -> hyper::Response<hyper::Body> {
        hyper::Response::builder()
            .status(status)
            .header(
                "content-type",
                hyper::header::HeaderValue::from_static(content_type),
            )
            .body(body.into())
            .expect("Unable to parse response body for type conversion")
    }
}

pub fn is_json(request: &hyper::Request<hyper::Body>) -> bool {
    request
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|val| val.to_str().ok())
        .map_or(false, |content| {
            content.eq_ignore_ascii_case("application/json")
                || content.eq_ignore_ascii_case("application/json; charset=utf-8")
                || content.eq_ignore_ascii_case("application/json;charset=utf-8")
        })
}
