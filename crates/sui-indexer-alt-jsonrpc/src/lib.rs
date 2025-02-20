// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context as _;
use api::checkpoints::Checkpoints;
use api::dynamic_fields::DynamicFields;
use api::move_utils::MoveUtils;
use api::name_service::NameService;
use api::objects::{Objects, ObjectsConfig, QueryObjects};
use api::rpc_module::RpcModule;
use api::transactions::{QueryTransactions, Transactions, TransactionsConfig};
use config::RpcConfig;
use data::system_package_task::{SystemPackageTask, SystemPackageTaskArgs};
use jsonrpsee::server::{BatchRequestConfig, RpcServiceBuilder, ServerBuilder};
use metrics::middleware::MetricsLayer;
use metrics::RpcMetrics;
use prometheus::Registry;
use serde_json::json;
use sui_name_service::NameServiceConfig;
use sui_open_rpc::Project;
use sui_pg_db::DbArgs;
use tokio::{join, signal, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tower_layer::Identity;
use tracing::info;

use crate::api::governance::Governance;
use crate::context::Context;

mod api;
pub mod args;
pub mod config;
mod context;
pub mod data;
mod error;
mod metrics;
mod paginate;

#[derive(clap::Args, Debug, Clone)]
pub struct RpcArgs {
    /// Address to listen to for incoming JSON-RPC connections.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,

    /// The maximum number of concurrent requests to accept. If the service receives more than this
    /// many requests, it will start responding with 429.
    #[clap(long, default_value_t = Self::default().max_in_flight_requests)]
    pub max_in_flight_requests: u32,
}

pub struct RpcService {
    /// The address that the server will start listening for requests on, when it is run.
    rpc_listen_address: SocketAddr,

    /// A partially built/configured JSON-RPC server.
    server: ServerBuilder<Identity, Identity>,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,

    /// All the methods added to the server so far.
    modules: jsonrpsee::RpcModule<()>,

    /// Description of the schema served by this service.
    schema: Project,

    /// Cancellation token controlling all services.
    cancel: CancellationToken,
}

impl RpcService {
    /// Create a new instance of the JSON-RPC service, configured by `rpc_args`. The service will
    /// not accept connections until [Self::run] is called.
    pub fn new(
        rpc_args: RpcArgs,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let RpcArgs {
            rpc_listen_address,
            max_in_flight_requests,
        } = rpc_args;

        let metrics = RpcMetrics::new(registry);

        let server = ServerBuilder::new()
            .http_only()
            // `jsonrpsee` calls this a limit on connections, but it is implemented as a limit on
            // requests.
            .max_connections(max_in_flight_requests)
            .max_response_body_size(u32::MAX)
            .set_batch_request_config(BatchRequestConfig::Disabled);

        let schema = Project::new(
            env!("CARGO_PKG_VERSION"),
            "Sui JSON-RPC",
            "A JSON-RPC API for interacting with the Sui blockchain.",
            "Mysten Labs",
            "https://mystenlabs.com",
            "build@mystenlabs.com",
            "Apache-2.0",
            "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE",
        );

        Ok(Self {
            rpc_listen_address,
            server,
            metrics,
            modules: jsonrpsee::RpcModule::new(()),
            schema,
            cancel,
        })
    }

    /// Return a copy of the metrics.
    pub fn metrics(&self) -> Arc<RpcMetrics> {
        self.metrics.clone()
    }

    /// Add an `RpcModule` to the service. The module's methods are combined with the existing
    /// methods registered on the service, and the operation will fail if there is any overlap.
    pub fn add_module(&mut self, module: impl RpcModule) -> anyhow::Result<()> {
        self.schema.add_module(module.schema());
        self.modules
            .merge(module.into_impl().remove_context())
            .context("Failed to add module because of a name conflict")
    }

    /// Start the service (it will accept connections) and return a handle that will resolve when
    /// the service stops.
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let Self {
            rpc_listen_address,
            server,
            metrics,
            mut modules,
            schema,
            cancel,
        } = self;

        info!("Starting JSON-RPC service on {rpc_listen_address}",);
        info!("Serving schema: {}", serde_json::to_string_pretty(&schema)?);

        // Add a method to serve the schema to clients.
        modules
            .register_method("rpc.discover", move |_, _, _| json!(schema.clone()))
            .context("Failed to add schema discovery method")?;

        let middleware = RpcServiceBuilder::new().layer(MetricsLayer::new(
            metrics,
            modules.method_names().map(|n| n.to_owned()).collect(),
        ));

        let handle = server
            .set_rpc_middleware(middleware)
            .build(rpc_listen_address)
            .await
            .context("Failed to bind JSON-RPC service")?
            .start(modules);

        // Set-up a helper task that will tear down the RPC service when the cancellation token is
        // triggered.
        let cancel_handle = handle.clone();
        let cancel_cancel = cancel.clone();
        let h_cancel = tokio::spawn(async move {
            cancel_cancel.cancelled().await;
            cancel_handle.stop()
        });

        // Set-up another helper task that will listen for Ctrl-C and trigger the cancellation
        // token.
        let ctrl_c_cancel = cancel.clone();
        let h_ctrl_c = tokio::spawn(async move {
            tokio::select! {
                _ = ctrl_c_cancel.cancelled() => {}
                _ = signal::ctrl_c() => {
                    ctrl_c_cancel.cancel();
                }
            }
        });

        Ok(tokio::spawn(async move {
            handle.stopped().await;
            cancel.cancel();
            let _ = join!(h_cancel, h_ctrl_c);
        }))
    }
}

impl Default for RpcArgs {
    fn default() -> Self {
        Self {
            rpc_listen_address: "0.0.0.0:6000".parse().unwrap(),
            max_in_flight_requests: 2000,
        }
    }
}

/// Set-up and run the RPC service, using the provided arguments (expected to be extracted from the
/// command-line). The service will continue to run until the cancellation token is triggered, and
/// will signal cancellation on the token when it is shutting down.
///
/// The service may spin up auxiliary services (such as the system package task) to support itself,
/// and will clean these up on shutdown as well.
pub async fn start_rpc(
    db_args: DbArgs,
    rpc_args: RpcArgs,
    system_package_task_args: SystemPackageTaskArgs,
    rpc_config: RpcConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let RpcConfig {
        objects,
        transactions,
        name_service,
        extra: _,
    } = rpc_config.finish();

    let objects_config = objects.finish(ObjectsConfig::default());
    let transactions_config = transactions.finish(TransactionsConfig::default());
    let name_service_config = name_service.finish(NameServiceConfig::default());

    let mut rpc = RpcService::new(rpc_args, registry, cancel.child_token())
        .context("Failed to create RPC service")?;

    let context = Context::new(db_args, rpc.metrics(), registry).await?;

    let system_package_task = SystemPackageTask::new(
        context.clone(),
        system_package_task_args,
        cancel.child_token(),
    );

    rpc.add_module(Checkpoints(context.clone()))?;
    rpc.add_module(DynamicFields(context.clone()))?;
    rpc.add_module(Governance(context.clone()))?;
    rpc.add_module(MoveUtils(context.clone()))?;
    rpc.add_module(NameService(context.clone(), name_service_config))?;
    rpc.add_module(Objects(context.clone(), objects_config.clone()))?;
    rpc.add_module(QueryObjects(context.clone(), objects_config))?;
    rpc.add_module(QueryTransactions(context.clone(), transactions_config))?;
    rpc.add_module(Transactions(context.clone()))?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;
    let h_system_package_task = system_package_task.run();

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
        let _ = h_system_package_task.await;
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        time::Duration,
    };

    use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::error::METHOD_NOT_FOUND_CODE};
    use reqwest::Client;
    use serde_json::{json, Value};
    use sui_open_rpc::Module;
    use sui_open_rpc_macros::open_rpc;
    use sui_pg_db::temp::get_available_port;

    use super::*;

    #[tokio::test]
    async fn test_add_module() {
        let mut rpc = test_service().await;

        rpc.add_module(Foo).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_multiple_methods() {
        let mut rpc = test_service().await;

        rpc.add_module(Bar).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_multiple_modules() {
        let mut rpc = test_service().await;

        rpc.add_module(Foo).unwrap();
        rpc.add_module(Baz).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_conflict() {
        let mut rpc = test_service().await;

        rpc.add_module(Foo).unwrap();
        assert!(rpc.add_module(Bar).is_err(),)
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let cancel = CancellationToken::new();
        let rpc = RpcService::new(
            RpcArgs {
                rpc_listen_address: test_listen_address(),
                ..Default::default()
            },
            &Registry::new(),
            cancel.clone(),
        )
        .unwrap();

        let handle = rpc.run().await.unwrap();

        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("Shutdown should not timeout")
            .expect("Shutdown should succeed");
    }

    #[tokio::test]
    async fn test_rpc_discovery() {
        let cancel = CancellationToken::new();
        let rpc_listen_address = test_listen_address();

        let mut rpc = RpcService::new(
            RpcArgs {
                rpc_listen_address,
                ..Default::default()
            },
            &Registry::new(),
            cancel.clone(),
        )
        .unwrap();

        rpc.add_module(Foo).unwrap();
        rpc.add_module(Baz).unwrap();

        let handle = rpc.run().await.unwrap();

        let url = format!("http://{}/", rpc_listen_address);
        let client = Client::new();

        let resp: Value = client
            .post(&url)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "rpc.discover",
                "id": 1,
            }))
            .send()
            .await
            .expect("Request should succeed")
            .json()
            .await
            .expect("Deserialization should succeed");

        assert_eq!(resp["result"]["info"]["title"], "Sui JSON-RPC");
        assert_eq!(
            resp["result"]["methods"],
            json!([
                {
                    "name": "test_bar",
                    "tags": [{
                        "name": "Test API"
                    }],
                    "params": [],
                    "result": {
                        "name": "u64",
                        "required": true,
                        "schema": {
                            "type": "integer",
                            "format": "uint64",
                            "minimum": 0.0
                        }
                    }
                },
                {
                    "name": "test_baz",
                    "tags": [{
                        "name": "Test API"
                    }],
                    "params": [],
                    "result": {
                        "name": "u64",
                        "required": true,
                        "schema": {
                            "type": "integer",
                            "format": "uint64",
                            "minimum": 0.0
                        }
                    }
                }
            ])
        );

        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("Shutdown should not timeout")
            .expect("Shutdown should succeed");
    }

    #[tokio::test]
    async fn test_request_metrics() {
        let cancel = CancellationToken::new();
        let rpc_listen_address = test_listen_address();

        let mut rpc = RpcService::new(
            RpcArgs {
                rpc_listen_address,
                ..Default::default()
            },
            &Registry::new(),
            cancel.clone(),
        )
        .unwrap();

        rpc.add_module(Foo).unwrap();

        let metrics = rpc.metrics();
        let handle = rpc.run().await.unwrap();

        let url = format!("http://{}/", rpc_listen_address);
        let client = Client::new();

        client
            .post(&url)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "test_bar",
                "id": 1,
            }))
            .send()
            .await
            .expect("Request should succeed");

        client
            .post(&url)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "test_baz",
                "id": 1,
            }))
            .send()
            .await
            .expect("Request should succeed");

        assert_eq!(
            metrics
                .requests_received
                .with_label_values(&["test_bar"])
                .get(),
            1
        );

        assert_eq!(
            metrics
                .requests_succeeded
                .with_label_values(&["test_bar"])
                .get(),
            1
        );

        assert_eq!(
            metrics
                .requests_received
                .with_label_values(&["<UNKNOWN>"])
                .get(),
            1
        );

        assert_eq!(
            metrics
                .requests_succeeded
                .with_label_values(&["<UNKNOWN>"])
                .get(),
            0
        );

        assert_eq!(
            metrics
                .requests_failed
                .with_label_values(&["<UNKNOWN>", &format!("{METHOD_NOT_FOUND_CODE}")])
                .get(),
            1
        );

        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("Shutdown should not timeout")
            .expect("Shutdown should succeed");
    }

    // Test Helpers

    #[open_rpc(namespace = "test", tag = "Test API")]
    #[rpc(server, namespace = "test")]
    trait FooApi {
        #[method(name = "bar")]
        fn bar(&self) -> RpcResult<u64>;
    }

    #[open_rpc(namespace = "test", tag = "Test API")]
    #[rpc(server, namespace = "test")]
    trait BarApi {
        #[method(name = "bar")]
        fn bar(&self) -> RpcResult<u64>;

        #[method(name = "baz")]
        fn baz(&self) -> RpcResult<u64>;
    }

    #[open_rpc(namespace = "test", tag = "Test API")]
    #[rpc(server, namespace = "test")]
    trait BazApi {
        #[method(name = "baz")]
        fn baz(&self) -> RpcResult<u64>;
    }

    struct Foo;
    struct Bar;
    struct Baz;

    impl FooApiServer for Foo {
        fn bar(&self) -> RpcResult<u64> {
            Ok(42)
        }
    }

    impl BarApiServer for Bar {
        fn bar(&self) -> RpcResult<u64> {
            Ok(43)
        }

        fn baz(&self) -> RpcResult<u64> {
            Ok(44)
        }
    }

    impl BazApiServer for Baz {
        fn baz(&self) -> RpcResult<u64> {
            Ok(45)
        }
    }

    impl RpcModule for Foo {
        fn schema(&self) -> Module {
            FooApiOpenRpc::module_doc()
        }

        fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
            self.into_rpc()
        }
    }

    impl RpcModule for Bar {
        fn schema(&self) -> Module {
            BarApiOpenRpc::module_doc()
        }

        fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
            self.into_rpc()
        }
    }

    impl RpcModule for Baz {
        fn schema(&self) -> Module {
            BazApiOpenRpc::module_doc()
        }

        fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
            self.into_rpc()
        }
    }

    fn test_listen_address() -> SocketAddr {
        let port = get_available_port();
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    async fn test_service() -> RpcService {
        let cancel = CancellationToken::new();
        RpcService::new(
            RpcArgs {
                rpc_listen_address: test_listen_address(),
                ..Default::default()
            },
            &Registry::new(),
            cancel,
        )
        .expect("Failed to create test JSON-RPC service")
    }
}
