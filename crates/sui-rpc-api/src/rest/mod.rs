// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{handler::Handler, http::Method, routing::MethodRouter};
use axum::{
    response::{Redirect, ResponseParts},
    routing::get,
    Router,
};

use crate::{reader::StateReader, RpcService};

pub mod accept;
pub mod accounts;
pub mod checkpoints;
pub mod content_type;
pub mod health;
pub mod info;
pub mod objects;
pub mod system;
pub mod transactions;

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

pub const ENDPOINTS: &[&dyn ApiEndpoint<RpcService>] = &[
    &info::GetNodeInfo,
    &health::HealthCheck,
    &checkpoints::GetCheckpoint,
    &accounts::ListAccountObjects,
    &objects::GetObject,
    &objects::GetObjectWithVersion,
    &objects::ListDynamicFields,
    &checkpoints::ListCheckpoints,
    &transactions::GetTransaction,
    &transactions::ListTransactions,
    &system::GetSystemStateSummary,
    &system::GetCurrentProtocolConfig,
    &system::GetProtocolConfig,
    &system::GetGasInfo,
    &transactions::ExecuteTransaction,
    &transactions::SimulateTransaction,
    &transactions::ResolveTransaction,
];

pub fn build_rest_router(service: RpcService) -> axum::Router {
    let mut api = Router::new();

    for endpoint in ENDPOINTS {
        let handler = endpoint.handler();
        assert_eq!(handler.method(), endpoint.method());

        // we need to replace any path parameters wrapped in braces to be prefaced by a colon
        // until axum updates matchit: https://github.com/tokio-rs/axum/pull/2645
        let path = endpoint.path().replace('{', ":").replace('}', "");

        api = api.route(&path, handler.handler);
    }

    Router::new()
        .nest("/v2/", api.with_state(service))
        .route("/v2", get(|| async { Redirect::permanent("/v2/") }))
        // Previously the service used to be hosted at `/rest`. In an effort to migrate folks
        // to the new versioned route, we'll issue redirects from `/rest` -> `/v2`.
        .route("/rest/*path", axum::routing::method_routing::any(redirect))
        .route("/rest", get(|| async { Redirect::permanent("/v2/") }))
        .route("/rest/", get(|| async { Redirect::permanent("/v2/") }))
}

pub struct PageCursor<C>(pub Option<C>);

impl<C: std::fmt::Display> axum::response::IntoResponseParts for PageCursor<C> {
    type Error = (axum::http::StatusCode, String);

    fn into_response_parts(
        self,
        res: ResponseParts,
    ) -> std::result::Result<ResponseParts, Self::Error> {
        self.0
            .map(|cursor| [(crate::types::X_SUI_CURSOR, cursor.to_string())])
            .into_response_parts(res)
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
    }
}

impl<C: std::fmt::Display> axum::response::IntoResponse for PageCursor<C> {
    fn into_response(self) -> axum::response::Response {
        (self, ()).into_response()
    }
}

pub const DEFAULT_PAGE_SIZE: usize = 50;
pub const MAX_PAGE_SIZE: usize = 100;

// Enable StateReader to be used as axum::extract::State
impl axum::extract::FromRef<RpcService> for StateReader {
    fn from_ref(input: &RpcService) -> Self {
        input.reader.clone()
    }
}

// Enable TransactionExecutor to be used as axum::extract::State
impl axum::extract::FromRef<RpcService>
    for Option<Arc<dyn sui_types::transaction_executor::TransactionExecutor>>
{
    fn from_ref(input: &RpcService) -> Self {
        input.executor.clone()
    }
}

async fn redirect(axum::extract::Path(path): axum::extract::Path<String>) -> Redirect {
    Redirect::permanent(&format!("/v2/{path}"))
}

pub trait ApiEndpoint<S> {
    fn method(&self) -> Method;
    fn path(&self) -> &'static str;
    fn handler(&self) -> RouteHandler<S>;
}

pub struct RouteHandler<S> {
    method: axum::http::Method,
    handler: MethodRouter<S>,
}

impl<S: Clone> RouteHandler<S> {
    pub fn new<H, T>(method: axum::http::Method, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
        S: Send + Sync + 'static,
    {
        let handler = MethodRouter::new().on(method.clone().try_into().unwrap(), handler);

        Self { method, handler }
    }

    pub fn method(&self) -> &axum::http::Method {
        &self.method
    }
}
