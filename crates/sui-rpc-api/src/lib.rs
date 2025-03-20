// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::callback::CallbackLayer;
use reader::StateReader;
use std::sync::Arc;
use subscription::SubscriptionServiceHandle;
use sui_types::storage::RpcStateReader;
use sui_types::transaction_executor::TransactionExecutor;
use tap::Pipe;

pub mod client;
mod config;
mod error;
pub mod field_mask;
mod grpc;
pub mod message;
mod metrics;
pub mod proto;
mod reader;
mod response;
mod service;
pub mod subscription;
pub mod types;

pub use client::Client;
pub use config::Config;
pub use error::{ErrorDetails, ErrorReason, Result, RpcError};
pub use metrics::RpcMetrics;

#[derive(Clone)]
pub struct RpcService {
    reader: StateReader,
    executor: Option<Arc<dyn TransactionExecutor>>,
    subscription_service_handle: Option<SubscriptionServiceHandle>,
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
            subscription_service_handle: None,
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

    pub fn with_subscription_service(
        &mut self,
        subscription_service_handle: SubscriptionServiceHandle,
    ) {
        self.subscription_service_handle = Some(subscription_service_handle);
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

        let router = {
            let ledger_service =
                crate::proto::rpc::v2beta::ledger_service_server::LedgerServiceServer::new(
                    self.clone(),
                );
            let transaction_execution_service = crate::proto::rpc::v2beta::transaction_execution_service_server::TransactionExecutionServiceServer::new(self.clone());
            let live_data_service =
                crate::proto::rpc::v2alpha::live_data_service_server::LiveDataServiceServer::new(
                    self.clone(),
                );

            let node_service =
                crate::proto::node::v2::node_service_server::NodeServiceServer::new(self.clone());

            let (mut health_reporter, health_service) = tonic_health::server::health_reporter();

            let reflection_v1 = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::proto::google::protobuf::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    crate::proto::google::rpc::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    crate::proto::rpc::v2beta::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(crate::proto::types::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::node::v2::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(
                    crate::proto::rpc::v2alpha::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1()
                .unwrap();

            let reflection_v1alpha = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::proto::google::protobuf::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    crate::proto::google::rpc::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    crate::proto::rpc::v2beta::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(crate::proto::types::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(crate::proto::node::v2::FILE_DESCRIPTOR_SET)
                .register_encoded_file_descriptor_set(
                    crate::proto::rpc::v2alpha::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1alpha()
                .unwrap();

            fn service_name<S: tonic::server::NamedService>(_service: &S) -> &'static str {
                S::NAME
            }

            for service_name in [
                service_name(&ledger_service),
                service_name(&transaction_execution_service),
                service_name(&live_data_service),
                service_name(&node_service),
                service_name(&reflection_v1),
                service_name(&reflection_v1alpha),
            ] {
                health_reporter
                    .set_service_status(service_name, tonic_health::ServingStatus::Serving)
                    .await;
            }

            let mut services = grpc::Services::new()
                .add_service(reflection_v1)
                .add_service(reflection_v1alpha)
                .add_service(ledger_service)
                .add_service(transaction_execution_service)
                .add_service(live_data_service)
                .add_service(node_service);

            if let Some(subscription_service_handle) = self.subscription_service_handle.clone() {
                let subscription_service =
crate::proto::rpc::v2alpha::subscription_service_server::SubscriptionServiceServer::new(subscription_service_handle);
                health_reporter
                    .set_service_status(
                        service_name(&subscription_service),
                        tonic_health::ServingStatus::Serving,
                    )
                    .await;
                services = services.add_service(subscription_service);
            }

            services.add_service(health_service).into_router()
        };

        let health_endpoint = axum::Router::new()
            .route("/health", axum::routing::get(service::health::health))
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
