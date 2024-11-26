// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::callback::CallbackLayer;
use reader::StateReader;
use rest::build_rest_router;
use std::sync::Arc;
use sui_types::storage::RpcStateReader;
use sui_types::transaction_executor::TransactionExecutor;
use tap::Pipe;

pub mod client;
mod config;
mod error;
mod metrics;
pub mod proto;
mod reader;
mod response;
pub mod rest;
pub mod types;

pub use client::Client;
pub use config::Config;
pub use error::{RestError, Result};
pub use metrics::RpcMetrics;
pub use rest::checkpoints::CheckpointResponse;
pub use rest::checkpoints::ListCheckpointsQueryParameters;
pub use rest::objects::ObjectResponse;
pub use rest::transactions::ExecuteTransactionQueryParameters;
pub use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};

#[derive(Clone)]
pub struct RestService {
    reader: StateReader,
    executor: Option<Arc<dyn TransactionExecutor>>,
    chain_id: sui_types::digests::ChainIdentifier,
    software_version: &'static str,
    metrics: Option<Arc<RpcMetrics>>,
    config: Config,
}

impl RestService {
    pub fn new(reader: Arc<dyn RpcStateReader>, software_version: &'static str) -> Self {
        let chain_id = reader.get_chain_identifier().unwrap();
        Self {
            reader: StateReader::new(reader),
            executor: None,
            chain_id,
            software_version,
            metrics: None,
            config: Config::default(),
        }
    }

    pub fn new_without_version(reader: Arc<dyn RpcStateReader>) -> Self {
        Self::new(reader, "unknown")
    }

    pub fn with_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn with_executor(&mut self, executor: Arc<dyn TransactionExecutor + Send + Sync>) {
        self.executor = Some(executor);
    }

    pub fn with_metrics(&mut self, metrics: RpcMetrics) {
        self.metrics = Some(Arc::new(metrics));
    }

    pub fn chain_id(&self) -> sui_types::digests::ChainIdentifier {
        self.chain_id
    }

    pub fn software_version(&self) -> &'static str {
        self.software_version
    }

    pub fn into_router(self) -> axum::Router {
        let metrics = self.metrics.clone();

        let rest_router = build_rest_router(self.clone());

        rest_router
            .layer(axum::middleware::map_response_with_state(
                self,
                response::append_info_headers,
            ))
            .pipe(|router| {
                if let Some(metrics) = metrics {
                    router.layer(CallbackLayer::new(
                        metrics::RpcMetricsMakeCallbackHandler::new(metrics),
                    ))
                } else {
                    router
                }
            })
    }

    pub async fn start_service(self, socket_address: std::net::SocketAddr) {
        let listener = tokio::net::TcpListener::bind(socket_address).await.unwrap();
        axum::serve(listener, self.into_router()).await.unwrap();
    }
}

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ascending,
    Descending,
}

impl Direction {
    pub fn is_descending(self) -> bool {
        matches!(self, Self::Descending)
    }
}
