// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use prometheus::Registry;
use sui_kvstore::BigTableClient;
use sui_kvstore::KeyValueStoreReader;
pub use sui_kvstore::PoolConfig;
use sui_package_resolver::PackageStore;
use sui_package_resolver::PackageStoreWithLruCache;
use sui_package_resolver::Resolver;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer;
use sui_rpc_api::ServerVersion;
use sui_types::digests::ChainIdentifier;
use sui_types::message_envelope::Message;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tokio::time::sleep;
use tonic::transport::Identity;
use tonic::transport::Server;
use tonic::transport::ServerTlsConfig;
use tracing::error;

mod package_store;
mod v2;

use package_store::BigTablePackageStore;

pub type PackageResolver = Arc<Resolver<Arc<dyn PackageStore>>>;

#[derive(Clone)]
pub struct KvRpcServer {
    chain_id: ChainIdentifier,
    client: BigTableClient,
    server_version: Option<ServerVersion>,
    checkpoint_bucket: Option<String>,
    cache: Arc<RwLock<Option<GetServiceInfoResponse>>>,
    package_resolver: PackageResolver,
}

/// Optional configuration for the gRPC server (TLS, metrics, reflection).
#[derive(Default)]
pub struct ServerConfig {
    pub tls_identity: Option<Identity>,
    pub metrics_registry: Option<Registry>,
    pub enable_reflection: bool,
}

impl KvRpcServer {
    pub async fn new(
        instance_id: String,
        project_id: Option<String>,
        app_profile_id: Option<String>,
        checkpoint_bucket: Option<String>,
        channel_timeout: Option<Duration>,
        server_version: Option<ServerVersion>,
        registry: &Registry,
        credentials_path: Option<String>,
        pool_config: PoolConfig,
    ) -> anyhow::Result<Self> {
        let mut client = BigTableClient::new_remote_with_credentials(
            instance_id,
            project_id,
            false,
            channel_timeout,
            None,
            "sui-kv-rpc".to_string(),
            Some(registry),
            app_profile_id,
            pool_config,
            credentials_path,
        )
        .await?;
        let genesis = client
            .get_checkpoints(&[0])
            .await?
            .pop()
            .expect("failed to fetch genesis checkpoint from the KV store");
        let chain_id = ChainIdentifier::from(genesis.summary.digest());
        Ok(Self::init(
            client,
            chain_id,
            server_version,
            checkpoint_bucket,
        ))
    }

    /// Construct a KvRpcServer backed by a local BigTable emulator.
    pub async fn new_local(
        host: String,
        instance_id: String,
        server_version: Option<ServerVersion>,
        checkpoint_bucket: Option<String>,
    ) -> anyhow::Result<Self> {
        let client = BigTableClient::new_local(host, instance_id).await?;
        Ok(Self::init(
            client,
            ChainIdentifier::from(sui_types::digests::CheckpointDigest::default()),
            server_version,
            checkpoint_bucket,
        ))
    }

    fn init(
        client: BigTableClient,
        chain_id: ChainIdentifier,
        server_version: Option<ServerVersion>,
        checkpoint_bucket: Option<String>,
    ) -> Self {
        let cache = Arc::new(RwLock::new(None));

        let package_store: Arc<dyn PackageStore> = Arc::new(PackageStoreWithLruCache::new(
            BigTablePackageStore::new(client.clone()),
        ));
        let package_resolver = Arc::new(Resolver::new(package_store));

        let server = Self {
            chain_id,
            client,
            server_version,
            checkpoint_bucket,
            cache,
            package_resolver,
        };

        let server_clone = server.clone();
        tokio::spawn(async move {
            loop {
                match v2::get_service_info(
                    server_clone.client.clone(),
                    server_clone.chain_id,
                    server_clone.server_version.clone(),
                )
                .await
                {
                    Ok(info) => {
                        let mut cache = server_clone.cache.write().await;
                        *cache = Some(info);
                    }
                    Err(e) => error!("Failed to update service info cache: {:?}", e),
                }
                sleep(Duration::from_millis(10)).await;
            }
        });

        server
    }

    /// Start this server as a tonic gRPC service on the given address.
    /// Returns a `Service` handle for lifecycle management.
    pub async fn start_service(
        self,
        listen_address: SocketAddr,
        config: ServerConfig,
    ) -> anyhow::Result<sui_futures::service::Service> {
        use mysten_network::callback::CallbackLayer;
        use sui_rpc_api::{RpcMetrics, RpcMetricsMakeCallbackHandler};

        let mut builder = Server::builder();

        if let Some(identity) = config.tls_identity {
            builder = builder.tls_config(ServerTlsConfig::new().identity(identity))?;
        }

        let registry = config.metrics_registry.unwrap_or_default();
        let mut router = builder
            .layer(CallbackLayer::new(RpcMetricsMakeCallbackHandler::new(
                Arc::new(RpcMetrics::new(&registry)),
            )))
            .add_service(LedgerServiceServer::new(self));

        if config.enable_reflection {
            let reflection_v1 = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    sui_rpc_api::proto::google::protobuf::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    sui_rpc_api::proto::google::rpc::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
                )
                .build_v1()?;
            let reflection_v1alpha = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    sui_rpc_api::proto::google::protobuf::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    sui_rpc_api::proto::google::rpc::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(
                    sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
                )
                .build_v1alpha()?;
            router = router
                .add_service(reflection_v1)
                .add_service(reflection_v1alpha);
        }

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server_future = router.serve_with_shutdown(listen_address, async {
            let _ = rx.await;
        });

        let service = sui_futures::service::Service::new()
            .spawn(async move {
                server_future.await?;
                Ok(())
            })
            .with_shutdown_signal(async move {
                let _ = tx.send(());
            });

        Ok(service)
    }
}
