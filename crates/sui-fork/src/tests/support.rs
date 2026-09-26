// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for the `#[path]`-included test modules.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_api::subscription::SubscriptionService;
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use sui_types::storage::RpcStateReader;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::method;

use crate::ForkingServiceClient;
use crate::context::Context;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::services::ServiceManager;
use crate::store::ForkStore;

/// Mock remote that reports every object lookup as "not found". Execution routinely probes dynamic
/// fields that exist nowhere, and those reads must see an authoritative remote miss rather than a
/// failed request, which fails the read instead of reading as absent. Other query shapes still
/// fail fast (404), preserving the harness rule that tests pre-populate everything else they need.
pub(crate) async fn absent_objects_gql_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("multiGetObjects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "multiGetObjects": [null] }
        })))
        .mount(&server)
        .await;
    server
}

/// In-process fork serving the canonical sui-rpc-api services and the forking service over gRPC.
/// It builds a Simulacrum from the genesis of a `NetworkConfig`, reads pre-fork state it lacks from
/// an upstream GraphQL URL, and starts a tonic server on an ephemeral port. The server task is
/// aborted when this is dropped.
pub(crate) struct ForkServer {
    pub(crate) grpc_endpoint: String,
    pub(crate) store: ForkStore,
    pub(crate) reference_gas_price: u64,
    server_task: tokio::task::JoinHandle<()>,
    _context: Arc<Context>,
    // Held to keep the metadata and RPC store directory alive for the server lifetime.
    _temp: tempfile::TempDir,
}

impl ForkServer {
    pub(crate) async fn start(config: &NetworkConfig, upstream_url: String) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let genesis_checkpoint = config.genesis.checkpoint();
        let genesis_contents = config.genesis.checkpoint_contents().clone();
        let forked_at_checkpoint = genesis_checkpoint.data().sequence_number;
        let chain_identifier = (*genesis_checkpoint.digest()).into();
        let services = ServiceManager::open(
            temp.path(),
            "localnet".to_owned(),
            forked_at_checkpoint,
            chain_identifier,
        )?;
        let mut store = ForkStore::new_for_testing_with_remote(
            temp.path().to_path_buf(),
            upstream_url,
            forked_at_checkpoint,
            services.local_store(),
        );
        store.save_checkpoint(&genesis_checkpoint, &genesis_contents)?;
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        store.update_objects(written, vec![]);

        let sim = Simulacrum::new_from_custom_state(
            KeyStore::from_network_config(config),
            genesis_checkpoint,
            config.genesis.sui_system_object(),
            chain_identifier,
            config,
            store.clone(),
            OsRng,
        );
        let reference_gas_price = sim.reference_gas_price();

        let registry = Registry::new();
        let (checkpoint_sender, subscription_handle) =
            SubscriptionService::build(&registry, None, None, None, None);
        // Service-backed on purpose: subscribers are published to by the indexer's broadcast
        // pipeline, so a service-less context would exercise a publication path production never
        // takes.
        let context = Arc::new(Context::new(sim, services, checkpoint_sender, &registry).await?);

        let reader: Arc<dyn RpcStateReader> = Arc::new(store.clone());
        let mut service = RpcService::new(reader);
        service.with_server_version(ServerVersion::new("sui-fork", "test"));
        service.with_subscription_service(subscription_handle);
        service.with_executor(Arc::new(ForkedTransactionExecutor::new(context.clone())));
        service.with_custom_service(ForkingServiceServer::new(ForkingServiceImpl::new(
            context.clone(),
        )));
        service.with_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET);

        // Bind to an ephemeral port via a probe listener, then drop it and let `start_service`
        // rebind. The window between is short enough not to matter for in-process tests.
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = probe.local_addr()?;
        drop(probe);
        let server_task = tokio::spawn(async move { service.start_service(addr).await });
        let grpc_endpoint = format!("http://{addr}");

        for _ in 0..50 {
            if ForkingServiceClient::connect(grpc_endpoint.clone())
                .await
                .is_ok()
            {
                return Ok(Self {
                    grpc_endpoint,
                    store,
                    reference_gas_price,
                    server_task,
                    _context: context,
                    _temp: temp,
                });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(anyhow!("timed out waiting for gRPC server to bind"))
    }
}

impl Drop for ForkServer {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}
