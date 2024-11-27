// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{
    response::{Redirect, ResponseParts},
    routing::get,
    Router,
};

use crate::{reader::StateReader, response, RpcService};
use openapi::ApiEndpoint;

pub mod accept;
pub mod accounts;
pub mod checkpoints;
pub mod coins;
mod committee;
pub mod content_type;
pub mod health;
pub mod info;
pub mod objects;
pub mod openapi;
pub mod system;
pub mod transactions;

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

pub const ENDPOINTS: &[&dyn ApiEndpoint<RpcService>] = &[
    // stable APIs
    &info::GetNodeInfo,
    &health::HealthCheck,
    &checkpoints::GetCheckpoint,
    // unstable APIs
    &accounts::ListAccountObjects,
    &objects::GetObject,
    &objects::GetObjectWithVersion,
    &objects::ListDynamicFields,
    &checkpoints::ListCheckpoints,
    &checkpoints::GetFullCheckpoint,
    &transactions::GetTransaction,
    &transactions::ListTransactions,
    &committee::GetCommittee,
    &committee::GetLatestCommittee,
    &system::GetSystemStateSummary,
    &system::GetCurrentProtocolConfig,
    &system::GetProtocolConfig,
    &system::GetGasInfo,
    &transactions::ExecuteTransaction,
    &transactions::SimulateTransaction,
    &transactions::ResolveTransaction,
    &coins::GetCoinInfo,
];

pub fn build_rest_router(service: RpcService) -> axum::Router {
    let mut api = openapi::Api::new(info(service.software_version()));

    api.register_endpoints(
        ENDPOINTS
            .iter()
            .copied()
            .filter(|endpoint| endpoint.stable() || service.config.enable_unstable_apis()),
    );

    Router::new()
        .nest("/v2/", api.to_router().with_state(service))
        .route("/v2", get(|| async { Redirect::permanent("/v2/") }))
        // Previously the service used to be hosted at `/rest`. In an effort to migrate folks
        // to the new versioned route, we'll issue redirects from `/rest` -> `/v2`.
        .route("/rest/*path", axum::routing::method_routing::any(redirect))
        .route("/rest", get(|| async { Redirect::permanent("/v2/") }))
        .route("/rest/", get(|| async { Redirect::permanent("/v2/") }))
}

#[derive(Debug)]
pub struct Page<T, C> {
    pub entries: response::ResponseContent<Vec<T>>,
    pub cursor: Option<C>,
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

impl<T: serde::Serialize, C: std::fmt::Display> axum::response::IntoResponse for Page<T, C> {
    fn into_response(self) -> axum::response::Response {
        let cursor = self
            .cursor
            .map(|cursor| [(crate::types::X_SUI_CURSOR, cursor.to_string())]);

        (cursor, self.entries).into_response()
    }
}

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

pub fn info(version: &'static str) -> openapiv3::v3_1::Info {
    use openapiv3::v3_1::Contact;
    use openapiv3::v3_1::License;

    openapiv3::v3_1::Info {
        title: "Sui Node Api".to_owned(),
        description: Some("REST Api for interacting with the Sui Blockchain".to_owned()),
        contact: Some(Contact {
            name: Some("Mysten Labs".to_owned()),
            url: Some("https://github.com/MystenLabs/sui".to_owned()),
            ..Default::default()
        }),
        license: Some(License {
            name: "Apache 2.0".to_owned(),
            url: Some("https://www.apache.org/licenses/LICENSE-2.0.html".to_owned()),
            ..Default::default()
        }),
        version: version.to_owned(),
        ..Default::default()
    }
}

async fn redirect(axum::extract::Path(path): axum::extract::Path<String>) -> Redirect {
    Redirect::permanent(&format!("/v2/{path}"))
}

pub(crate) mod _schemars {
    use schemars::schema::InstanceType;
    use schemars::schema::Metadata;
    use schemars::schema::SchemaObject;
    use schemars::JsonSchema;

    pub(crate) struct U64;

    impl JsonSchema for U64 {
        fn schema_name() -> String {
            "u64".to_owned()
        }

        fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
            SchemaObject {
                metadata: Some(Box::new(Metadata {
                    description: Some("Radix-10 encoded 64-bit unsigned integer".to_owned()),
                    ..Default::default()
                })),
                instance_type: Some(InstanceType::String.into()),
                format: Some("u64".to_owned()),
                ..Default::default()
            }
            .into()
        }

        fn is_referenceable() -> bool {
            false
        }
    }
}
