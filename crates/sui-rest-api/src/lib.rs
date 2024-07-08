// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::Router;
use mysten_network::callback::CallbackLayer;
use openapi::ApiEndpoint;
use reader::StateReader;
use std::sync::Arc;
use sui_types::storage::RestStateReader;
use tap::Pipe;

pub mod accept;
mod accounts;
mod checkpoints;
pub mod client;
mod coins;
mod committee;
pub mod content_type;
mod error;
mod health;
mod info;
mod metrics;
mod objects;
pub mod openapi;
mod reader;
mod response;
mod system;
pub mod transactions;
pub mod types;

pub use client::Client;
pub use error::{RestError, Result};
pub use metrics::RestMetrics;
pub use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
pub use transactions::{ExecuteTransactionQueryParameters, TransactionExecutor};

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ascending,
    Descending,
}

pub struct Page<T, C> {
    pub entries: response::ResponseContent<Vec<T>>,
    pub cursor: Option<C>,
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

const ENDPOINTS: &[&dyn ApiEndpoint<RestService>] = &[
    &info::GetNodeInfo,
    &health::HealthCheck,
    &accounts::ListAccountObjects,
    &objects::GetObject,
    &objects::GetObjectWithVersion,
    &objects::ListDynamicFields,
    &checkpoints::ListCheckpoints,
    &checkpoints::GetCheckpoint,
    &checkpoints::GetCheckpointFull,
    &transactions::GetTransaction,
    &transactions::ListTransactions,
    &committee::GetCommittee,
    &committee::GetLatestCommittee,
    &system::GetSystemStateSummary,
    &system::GetCurrentProtocolConfig,
    &system::GetProtocolConfig,
    &system::GetGasInfo,
    &transactions::ExecuteTransaction,
    &coins::GetCoinInfo,
];

#[derive(Clone)]
pub struct RestService {
    reader: StateReader,
    executor: Option<Arc<dyn TransactionExecutor>>,
    chain_id: sui_types::digests::ChainIdentifier,
    software_version: &'static str,
    metrics: Option<Arc<RestMetrics>>,
}

impl axum::extract::FromRef<RestService> for StateReader {
    fn from_ref(input: &RestService) -> Self {
        input.reader.clone()
    }
}

impl axum::extract::FromRef<RestService> for Option<Arc<dyn TransactionExecutor>> {
    fn from_ref(input: &RestService) -> Self {
        input.executor.clone()
    }
}

impl RestService {
    pub fn new(reader: Arc<dyn RestStateReader>, software_version: &'static str) -> Self {
        let chain_id = reader.get_chain_identifier().unwrap();
        Self {
            reader: StateReader::new(reader),
            executor: None,
            chain_id,
            software_version,
            metrics: None,
        }
    }

    pub fn new_without_version(reader: Arc<dyn RestStateReader>) -> Self {
        Self::new(reader, "unknown")
    }

    pub fn with_executor(&mut self, executor: Arc<dyn TransactionExecutor + Send + Sync>) {
        self.executor = Some(executor);
    }

    pub fn with_metrics(&mut self, metrics: RestMetrics) {
        self.metrics = Some(Arc::new(metrics));
    }

    pub fn chain_id(&self) -> sui_types::digests::ChainIdentifier {
        self.chain_id
    }

    pub fn software_version(&self) -> &'static str {
        self.software_version
    }

    pub fn into_router(self) -> Router {
        let metrics = self.metrics.clone();

        let mut api = openapi::Api::new(info());

        api.register_endpoints(ENDPOINTS.to_owned());

        api.to_router()
            .with_state(self.clone())
            .layer(axum::middleware::map_response_with_state(
                self,
                response::append_info_headers,
            ))
            .pipe(|router| {
                if let Some(metrics) = metrics {
                    router.layer(CallbackLayer::new(
                        metrics::RestMetricsMakeCallbackHandler::new(metrics),
                    ))
                } else {
                    router
                }
            })
    }

    pub async fn start_service(self, socket_address: std::net::SocketAddr, base: Option<String>) {
        let mut app = self.into_router();

        if let Some(base) = base {
            app = Router::new().nest(&base, app);
        }

        axum::Server::bind(&socket_address)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }
}

fn info() -> openapiv3::v3_1::Info {
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
        version: "0.0.0".to_owned(),
        ..Default::default()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn openapi_spec() {
        const OPENAPI_SPEC_FILE: &str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/openapi/openapi.json");

        let openapi = {
            let mut api = openapi::Api::new(info());

            api.register_endpoints(ENDPOINTS.to_owned());
            api.openapi()
        };

        let mut actual = serde_json::to_string_pretty(&openapi).unwrap();
        actual.push('\n');

        // Update the expected format
        if std::env::var_os("UPDATE").is_some() {
            std::fs::write(OPENAPI_SPEC_FILE, &actual).unwrap();
        }

        let expected = std::fs::read_to_string(OPENAPI_SPEC_FILE).unwrap();

        let diff = diffy::create_patch(&expected, &actual);

        if !diff.hunks().is_empty() {
            let formatter = if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                diffy::PatchFormatter::new().with_color()
            } else {
                diffy::PatchFormatter::new()
            };
            let header = "Generated and checked-in openapi spec does not match. \
                          Re-run with `UPDATE=1` to update expected format";
            panic!("{header}\n\n{}", formatter.fmt_patch(&diff));
        }
    }

    #[tokio::test]
    async fn openapi_explorer() {
        // Unless env var is set, just early return
        if std::env::var_os("OPENAPI_EXPLORER").is_none() {
            return;
        }

        let openapi = {
            let mut api = openapi::Api::new(info());
            api.register_endpoints(ENDPOINTS.to_owned());
            api.openapi()
        };

        let router = openapi::OpenApiDocument::new(openapi).into_router();

        axum::Server::bind(&"127.0.0.1:8000".parse().unwrap())
            .serve(router.into_make_service())
            .await
            .unwrap();
    }
}
