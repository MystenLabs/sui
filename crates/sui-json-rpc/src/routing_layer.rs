// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::is_json;
use crate::CLIENT_TARGET_API_VERSION_HEADER;
use hyper::body::Bytes;
use hyper::{body, http, Body, Request, Response};
use jsonrpsee::core::__reexports::serde_json;
use jsonrpsee::core::server::helpers::MethodResponse;
use jsonrpsee::types::error::ErrorCode;
use jsonrpsee::types::{ErrorObject, Request as RpcRequest};
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
        let route_to_methods = routes.iter().map(|(_, v)| v.route_to.clone()).collect();
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

            let req = if is_json(req.headers().get(http::header::CONTENT_TYPE)) {
                let (part, body) = req.into_parts();
                let bytes = body::to_bytes(body).await?;
                let mut request: RpcRequest = serde_json::from_slice(&bytes)?;

                // Reject direct access to the old methods
                if route_to_methods.contains(request.method.as_ref()) {
                    let error = MethodResponse::error(
                        request.id,
                        ErrorObject::from(ErrorCode::MethodNotFound),
                    );
                    let response = hyper::Response::builder()
                        .status(hyper::StatusCode::OK)
                        .header(
                            "content-type",
                            hyper::header::HeaderValue::from_static(
                                "application/json; charset=utf-8",
                            ),
                        )
                        .body(Body::from(error.result))?;
                    return Ok(response);
                }

                // Modify the method name if routing is enabled
                if !disable_routing {
                    if let Some(version) = version {
                        if let Some(route) = routes.get(request.method.as_ref()) {
                            if route.matches(&version) {
                                request.method = route.route_to.clone().into();
                            }
                        };
                    }
                }

                let bytes = Bytes::from(serde_json::to_vec(&request)?);
                Request::from_parts(part, Body::from(bytes))
            } else {
                req
            };
            inner.call(req).await.map_err(|err| err.into())
        };
        Box::pin(res_fut)
    }
}
