// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use api::checkpoints::Checkpoints;
use api::coin::{Coins, DelegationCoins};
use api::dynamic_fields::DynamicFields;
use api::move_utils::MoveUtils;
use api::name_service::NameService;
use api::objects::{Objects, QueryObjects};
use api::rpc_module::RpcModule;
use api::transactions::{QueryTransactions, Transactions};
use api::write::Write;
use config::RpcConfig;
use jsonrpsee::server::{BatchRequestConfig, RpcServiceBuilder, ServerBuilder};
use metrics::RpcMetrics;
use metrics::middleware::MetricsLayer;
use prometheus::Registry;
use serde_json::json;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::{SystemPackageTask, SystemPackageTaskArgs};
use sui_open_rpc::Project;
use timeout::TimeoutLayer;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_layer::Identity;
use tracing::{info, warn};
use url::Url;

use crate::api::governance::{DelegationGovernance, Governance};
use crate::context::Context;

pub mod api;
pub mod args;
pub mod config;
mod context;
pub mod data;
mod error;
mod metrics;
mod paginate;
mod timeout;

#[derive(clap::Args, Debug, Clone)]
pub struct RpcArgs {
    /// Address to listen to for incoming JSON-RPC connections.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,

    /// The maximum number of concurrent requests to accept. If the service receives more than this
    /// many requests, it will start responding with 429.
    #[clap(long, default_value_t = Self::default().max_in_flight_requests)]
    pub max_in_flight_requests: u32,

    /// Requests that take longer than this (in milliseconds) to respond to will be terminated, and
    /// the query itself will be logged as a warning.
    #[clap(long, default_value_t = Self::default().request_timeout_ms)]
    pub request_timeout_ms: u64,

    /// Requests that take longer than this (in milliseconds) will be logged even if they succeed.
    /// This should be shorter than `request_timeout_ms`.
    #[clap(long, default_value_t = Self::default().slow_request_threshold_ms)]
    pub slow_request_threshold_ms: u64,
}

pub struct RpcService {
    /// The address that the server will start listening for requests on, when it is run.
    rpc_listen_address: SocketAddr,

    /// A partially built/configured JSON-RPC server.
    server: ServerBuilder<Identity, Identity>,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,

    /// Maximum time a request can take to complete.
    request_timeout: Duration,

    /// Threshold for logging slow requests.
    slow_request_threshold: Duration,

    /// All the methods added to the server so far.
    modules: jsonrpsee::RpcModule<()>,

    /// Description of the schema served by this service.
    schema: Project,

    /// Cancellation token controlling all services.
    cancel: CancellationToken,
}

impl RpcArgs {
    /// Requests that take longer than this are terminated and logged for debugging.
    fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }

    /// Requests that take longer than this are logged for debugging even if they succeed.
    /// This threshold should be lower than the request timeout threshold.
    fn slow_request_threshold(&self) -> Duration {
        Duration::from_millis(self.slow_request_threshold_ms)
    }
}

impl RpcService {
    /// Create a new instance of the JSON-RPC service, configured by `rpc_args`. The service will
    /// not accept connections until [Self::run] is called.
    pub fn new(
        rpc_args: RpcArgs,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let metrics = RpcMetrics::new(registry);

        let server = ServerBuilder::new()
            .http_only()
            // `jsonrpsee` calls this a limit on connections, but it is implemented as a limit on
            // requests.
            .max_connections(rpc_args.max_in_flight_requests)
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
            rpc_listen_address: rpc_args.rpc_listen_address,
            server,
            metrics,
            request_timeout: rpc_args.request_timeout(),
            slow_request_threshold: rpc_args.slow_request_threshold(),
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
            request_timeout,
            slow_request_threshold,
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

        let middleware = RpcServiceBuilder::new()
            .layer(TimeoutLayer::new(request_timeout))
            .layer(MetricsLayer::new(
                metrics,
                modules.method_names().map(|n| n.to_owned()).collect(),
                slow_request_threshold,
            ));

        let handle = server
            .set_rpc_middleware(middleware)
            .set_http_middleware(
                tower::builder::ServiceBuilder::new().layer(
                    tower_http::cors::CorsLayer::new()
                        .allow_methods([http::Method::GET, http::Method::POST])
                        .allow_origin(tower_http::cors::Any)
                        .allow_headers(tower_http::cors::Any),
                ),
            )
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

        Ok(tokio::spawn(async move {
            handle.stopped().await;
            cancel.cancel();
            let _ = h_cancel.await;
        }))
    }
}

impl Default for RpcArgs {
    fn default() -> Self {
        Self {
            rpc_listen_address: "0.0.0.0:6000".parse().unwrap(),
            max_in_flight_requests: 2000,
            request_timeout_ms: 60_000,
            slow_request_threshold_ms: 15_000,
        }
    }
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct NodeArgs {
    /// The URL of the fullnode RPC we connect to for transaction execution,
    /// dry-running, and delegation coin queries etc.
    #[arg(long)]
    pub fullnode_rpc_url: Option<url::Url>,
}

/// Set-up and run the RPC service, using the provided arguments (expected to be extracted from the
/// command-line). The service will continue to run until the cancellation token is triggered, and
/// will signal cancellation on the token when it is shutting down.
///
/// Access to most reads is controlled by the `database_url` -- if it is `None`, reads will not work.
/// The only exceptions are the `DelegationCoins` and `DelegationGovernance` modules, which are controlled
/// by `node_args.fullnode_rpc_url`, which can be omitted to disable reads from this RPC.
///
/// KV queries can optionally be served by a Bigtable instance, if `bigtable_instance` is provided.
/// Otherwise these requests are served by the database. If a `bigtable_instance` is provided, the
/// `GOOGLE_APPLICATION_CREDENTIALS` environment variable must point to the credentials JSON file.
///
/// Access to writes (executing and dry-running transactions) is controlled by `node_args.fullnode_rpc_url`,
/// which can be omitted to disable writes from this RPC.
///
/// The service may spin up auxiliary services (such as the system package task) to support itself,
/// and will clean these up on shutdown as well.
pub async fn start_rpc(
    database_url: Option<Url>,
    bigtable_instance: Option<String>,
    db_args: DbArgs,
    bigtable_args: BigtableArgs,
    rpc_args: RpcArgs,
    node_args: NodeArgs,
    system_package_task_args: SystemPackageTaskArgs,
    rpc_config: RpcConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let mut rpc = RpcService::new(rpc_args, registry, cancel.child_token())
        .context("Failed to create RPC service")?;

    let context = Context::new(
        database_url,
        bigtable_instance,
        db_args,
        bigtable_args,
        rpc_config,
        rpc.metrics(),
        registry,
        cancel.child_token(),
    )
    .await?;

    let system_package_task = SystemPackageTask::new(
        system_package_task_args,
        context.pg_reader().clone(),
        context.package_resolver().package_store().clone(),
        cancel.child_token(),
    );

    rpc.add_module(Checkpoints(context.clone()))?;
    rpc.add_module(Coins(context.clone()))?;
    rpc.add_module(DynamicFields(context.clone()))?;
    rpc.add_module(Governance(context.clone()))?;
    rpc.add_module(MoveUtils(context.clone()))?;
    rpc.add_module(NameService(context.clone()))?;
    rpc.add_module(Objects(context.clone()))?;
    rpc.add_module(QueryObjects(context.clone()))?;
    rpc.add_module(QueryTransactions(context.clone()))?;
    rpc.add_module(Transactions(context.clone()))?;

    if let Some(fullnode_rpc_url) = node_args.fullnode_rpc_url {
        rpc.add_module(DelegationCoins::new(
            fullnode_rpc_url.clone(),
            context.config().node.clone(),
        )?)?;
        rpc.add_module(DelegationGovernance::new(
            fullnode_rpc_url.clone(),
            context.config().node.clone(),
        )?)?;
        rpc.add_module(Write::new(fullnode_rpc_url, context.config().node.clone())?)?;
    } else {
        warn!(
            "No fullnode rpc url provided, DelegationCoins, DelegationGovernance, and Write modules will not be added."
        );
    }

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
    use serde_json::{Value, json};
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
                .with_label_values(&["UNKNOWN:test_baz"])
                .get(),
            1
        );

        assert_eq!(
            metrics
                .requests_succeeded
                .with_label_values(&["UNKNOWN:test_baz"])
                .get(),
            0
        );

        assert_eq!(
            metrics
                .requests_failed
                .with_label_values(&["UNKNOWN:test_baz", &format!("{METHOD_NOT_FOUND_CODE}")])
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
