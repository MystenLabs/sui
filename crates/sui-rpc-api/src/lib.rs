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
pub mod grpc;
mod metrics;
mod reader;
mod response;
mod service;
pub mod subscription;

pub use client::Client;
pub use config::Config;
pub use error::{
    CheckpointNotFoundError, ErrorDetails, ErrorReason, ObjectNotFoundError, Result, RpcError,
};
pub use metrics::{RpcMetrics, RpcMetricsMakeCallbackHandler};
pub use reader::TransactionNotFoundError;
pub use sui_rpc::proto;

#[derive(Clone)]
pub struct ServerVersion {
    pub bin: &'static str,
    pub version: &'static str,
}

impl ServerVersion {
    pub fn new(bin: &'static str, version: &'static str) -> Self {
        Self { bin, version }
    }
}

impl std::fmt::Display for ServerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.bin)?;
        f.write_str("/")?;
        f.write_str(self.version)
    }
}

#[derive(Clone)]
pub struct RpcService {
    reader: StateReader,
    executor: Option<Arc<dyn TransactionExecutor>>,
    subscription_service_handle: Option<SubscriptionServiceHandle>,
    chain_id: sui_types::digests::ChainIdentifier,
    server_version: Option<ServerVersion>,
    metrics: Option<Arc<RpcMetrics>>,
    config: Config,
}

impl RpcService {
    pub fn new(reader: Arc<dyn RpcStateReader>) -> Self {
        let chain_id = reader.get_chain_identifier().unwrap();
        Self {
            reader: StateReader::new(reader),
            executor: None,
            subscription_service_handle: None,
            chain_id,
            server_version: None,
            metrics: None,
            config: Config::default(),
        }
    }

    pub fn with_server_version(&mut self, server_version: ServerVersion) -> &mut Self {
        self.server_version = Some(server_version);
        self
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

    pub fn server_version(&self) -> Option<&ServerVersion> {
        self.server_version.as_ref()
    }

    pub async fn into_router(self) -> axum::Router {
        let metrics = self.metrics.clone();

        let router = {
            let ledger_service =
                sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer::new(
                    self.clone(),
                )
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);
            let transaction_execution_service = sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionServiceServer::new(self.clone())
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);
            let state_service =
                sui_rpc::proto::sui::rpc::v2::state_service_server::StateServiceServer::new(
                    self.clone(),
                )
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);
            let signature_verification_service = sui_rpc::proto::sui::rpc::v2::signature_verification_service_server::SignatureVerificationServiceServer::new(self.clone())
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);
            let move_package_service = sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageServiceServer::new(self.clone())
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);
            let name_service =
                sui_rpc::proto::sui::rpc::v2::name_service_server::NameServiceServer::new(
                    self.clone(),
                )
                .send_compressed(tonic::codec::CompressionEncoding::Zstd);

            let event_service_alpha =
                crate::grpc::alpha::event_service_proto::event_service_server::EventServiceServer::new(
                    self.clone(),
                );

            let (health_reporter, health_service) = tonic_health::server::health_reporter();

            let reflection_v1 = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::proto::google::protobuf::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    crate::proto::google::rpc::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
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
                    sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
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
                service_name(&state_service),
                service_name(&signature_verification_service),
                service_name(&move_package_service),
                service_name(&name_service),
                service_name(&event_service_alpha),
                service_name(&reflection_v1),
                service_name(&reflection_v1alpha),
            ] {
                health_reporter
                    .set_service_status(service_name, tonic_health::ServingStatus::Serving)
                    .await;
            }

            let mut services = grpc::Services::new()
                // V2
                .add_service(ledger_service)
                .add_service(transaction_execution_service)
                .add_service(state_service)
                .add_service(signature_verification_service)
                .add_service(move_package_service)
                .add_service(name_service)
                // alpha
                .add_service(event_service_alpha)
                // Reflection
                .add_service(reflection_v1)
                .add_service(reflection_v1alpha);

            if self.subscription_service_handle.is_some() {
                let subscription_service =
sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionServiceServer::new(self.clone());
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
