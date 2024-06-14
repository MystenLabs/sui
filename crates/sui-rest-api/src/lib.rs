// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    routing::{get, post},
    Router,
};

pub mod accept;
mod accounts;
mod checkpoints;
pub mod client;
mod committee;
pub mod content_type;
mod error;
mod health;
mod info;
mod metrics;
mod objects;
mod reader;
mod response;
mod system;
pub mod transactions;
pub mod types;

pub use client::Client;
pub use error::{RestError, Result};
pub use metrics::RestMetrics;
use mysten_network::callback::CallbackLayer;
use reader::StateReader;
use std::sync::Arc;
pub use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::storage::RestStateReader;
use tap::Pipe;
pub use transactions::{ExecuteTransactionQueryParameters, TransactionExecutor};

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
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
        let executor = self.executor.clone();
        let metrics = self.metrics.clone();

        Router::new()
            .route("/", get(info::node_info))
            .route(health::HEALTH_PATH, get(health::health))
            .route(
                accounts::LIST_ACCOUNT_OWNED_OBJECTS_PATH,
                get(accounts::list_account_owned_objects),
            )
            .route(
                transactions::GET_TRANSACTION_PATH,
                get(transactions::get_transaction),
            )
            .route(
                transactions::LIST_TRANSACTIONS_PATH,
                get(transactions::list_transactions),
            )
            .route(
                committee::GET_LATEST_COMMITTEE_PATH,
                get(committee::get_latest_committee),
            )
            .route(committee::GET_COMMITTEE_PATH, get(committee::get_committee))
            .route(
                system::GET_SYSTEM_STATE_SUMMARY_PATH,
                get(system::get_system_state_summary),
            )
            .route(
                system::GET_CURRENT_PROTOCOL_CONFIG_PATH,
                get(system::get_current_protocol_config),
            )
            .route(
                system::GET_PROTOCOL_CONFIG_PATH,
                get(system::get_protocol_config),
            )
            .route(system::GET_GAS_INFO_PATH, get(system::get_gas_info))
            .route(
                checkpoints::LIST_CHECKPOINT_PATH,
                get(checkpoints::list_checkpoints),
            )
            .route(
                checkpoints::GET_CHECKPOINT_PATH,
                get(checkpoints::get_checkpoint),
            )
            .route(
                checkpoints::GET_FULL_CHECKPOINT_PATH,
                get(checkpoints::get_full_checkpoint),
            )
            .route(objects::GET_OBJECT_PATH, get(objects::get_object))
            .route(
                objects::GET_OBJECT_WITH_VERSION_PATH,
                get(objects::get_object_with_version),
            )
            .with_state(self.clone())
            .pipe(|router| {
                if let Some(executor) = executor {
                    router.merge(execution_router(executor))
                } else {
                    router
                }
            })
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

fn execution_router(executor: Arc<dyn TransactionExecutor>) -> Router {
    Router::new()
        .route(
            transactions::POST_EXECUTE_TRANSACTION_PATH,
            post(transactions::execute_transaction),
        )
        .with_state(executor)
}
