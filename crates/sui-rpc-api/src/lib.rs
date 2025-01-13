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
mod grpc;
mod metrics;
pub mod proto;
mod reader;
mod response;
pub mod rest;
mod service;
pub mod types;

pub use client::Client;
pub use config::Config;
pub use error::{Result, RpcServiceError};
pub use metrics::RpcMetrics;
pub use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
pub use types::CheckpointResponse;
pub use types::ObjectResponse;

#[derive(Clone)]
pub struct RpcService {
    reader: StateReader,
    executor: Option<Arc<dyn TransactionExecutor>>,
    chain_id: sui_types::digests::ChainIdentifier,
    software_version: &'static str,
    metrics: Option<Arc<RpcMetrics>>,
    config: Config,
}

impl RpcService {
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

    pub async fn into_router(self) -> axum::Router {
        let metrics = self.metrics.clone();

        let mut router = {
            let node_service =
                crate::proto::node::node_service_server::NodeServiceServer::new(self.clone());
            // legacy node service
            let node = crate::proto::node::node_server::NodeServer::new(self.clone());

            let (mut health_reporter, health_service) = tonic_health::server::health_reporter();

            let reflection_v1 = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(crate::proto::google::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::types::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::node::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1()
                .unwrap();

            let reflection_v1alpha = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(crate::proto::google::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::types::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::node::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1alpha()
                .unwrap();

            fn service_name<S: tonic::server::NamedService>(_service: &S) -> &'static str {
                S::NAME
            }

            health_reporter
                .set_service_status(
                    service_name(&node_service),
                    tonic_health::ServingStatus::Serving,
                )
                .await;

            grpc::Services::new()
                .add_service(health_service)
                .add_service(reflection_v1)
                .add_service(reflection_v1alpha)
                .add_service(node_service)
                .add_service(node)
                .into_router()
        };

        if self.config.enable_experimental_rest_api() {
            router = router.merge(build_rest_router(self.clone()));
        }

        let health_endpoint = axum::Router::new()
            .route("/health", axum::routing::get(rest::health::health))
            .with_state(self.clone());

        router
            .merge(health_endpoint)
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
        axum::serve(listener, self.into_router().await)
            .await
            .unwrap();
    }
}

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
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
