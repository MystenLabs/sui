// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use jsonrpsee::server::{RpcModule, RpcServiceBuilder, Server, ServerBuilder};
use metrics::middleware::MetricsLayer;
use metrics::{MetricsService, RpcMetrics};
use sui_pg_db::Db;
use tokio::join;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_layer::{Identity, Stack};
use tracing::info;

use crate::api::governance::{GovernanceImpl, GovernanceServer};
use crate::args::Args;

mod api;
pub mod args;
mod metrics;

#[derive(clap::Args, Debug, Clone)]
pub struct RpcArgs {
    /// Address to listen to for incoming JSON-RPC connections.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    rpc_listen_address: SocketAddr,

    /// Address to serve Prometheus metrics from.
    #[clap(long, default_value_t = Self::default().metrics_address)]
    metrics_address: SocketAddr,

    /// The maximum number of concurrent connections to accept.
    #[clap(long, default_value_t = Self::default().max_rpc_connections)]
    max_rpc_connections: u32,
}

pub struct RpcService {
    /// The JSON-RPC server.
    server: Server<Identity, Stack<MetricsLayer, Identity>>,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,

    /// Service for serving Prometheus metrics.
    metrics_service: MetricsService,

    /// All the methods added to the server so far.
    modules: RpcModule<()>,

    /// Cancellation token controlling all services.
    cancel: CancellationToken,
}

impl RpcService {
    /// Create a new instance of the JSON-RPC service, configured by `rpc_args`. The service will
    /// bind to its socket upon construction (and will fail if this is not possible), but will not
    /// accept connections until [Self::run] is called.
    pub async fn new(rpc_args: RpcArgs, cancel: CancellationToken) -> anyhow::Result<Self> {
        let RpcArgs {
            rpc_listen_address,
            metrics_address,
            max_rpc_connections,
        } = rpc_args;

        let (metrics, metrics_service) = MetricsService::new(metrics_address, cancel.clone())
            .context("Failed to create metrics service")?;

        let middleware = RpcServiceBuilder::new().layer(MetricsLayer(metrics.clone()));

        let server = ServerBuilder::new()
            .http_only()
            .max_connections(max_rpc_connections)
            .set_rpc_middleware(middleware)
            .build(rpc_listen_address)
            .await
            .context("Failed to bind server")?;

        Ok(Self {
            server,
            metrics,
            metrics_service,
            modules: RpcModule::new(()),
            cancel,
        })
    }

    /// Return a copy of the metrics to inspect for testing purposes.
    pub fn metrics_for_test(&self) -> Arc<RpcMetrics> {
        self.metrics.clone()
    }

    /// Add an [RpcModule] to the service. The module's methods are combined with the existing
    /// methods registered on the service, and the operation will fail if there is any overlap.
    pub fn add_module<M>(&mut self, module: RpcModule<M>) -> anyhow::Result<()> {
        self.modules
            .merge(module.remove_context())
            .context("Failed to add module because of a name conflict")
    }

    /// Start the service (it will accept connections) and return a handle that will resolve when
    /// the service stops.
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let Self {
            server,
            metrics: _,
            metrics_service,
            modules,
            cancel,
        } = self;

        let listen_address = server
            .local_addr()
            .context("Can't start RPC service without listen address")?;

        let h_metrics = metrics_service
            .run()
            .await
            .context("Failed to start metrics service")?;

        info!("Starting JSON-RPC service on {listen_address}",);
        let handle = server.start(modules);

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
            let _ = join!(h_cancel, h_metrics);
        }))
    }
}

impl Default for RpcArgs {
    fn default() -> Self {
        Self {
            rpc_listen_address: "0.0.0.0:6000".parse().unwrap(),
            metrics_address: "0.0.0.0:9184".parse().unwrap(),
            max_rpc_connections: 100,
        }
    }
}

pub async fn start_rpc(args: Args) -> anyhow::Result<()> {
    let Args { db_args, rpc_args } = args;

    let cancel = CancellationToken::new();
    let mut rpc = RpcService::new(rpc_args, cancel)
        .await
        .context("Failed to start RPC service")?;

    let db = Db::for_read(db_args)
        .await
        .context("Failed to connect to database")?;

    rpc.add_module(GovernanceImpl(db.clone()).into_rpc())?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;
    let _ = h_rpc.await;
    Ok(())
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
    use serde_json::json;
    use sui_pg_db::temp::get_available_port;

    use super::*;

    fn test_listen_address() -> SocketAddr {
        let port = get_available_port();
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    async fn test_service() -> RpcService {
        let cancel = CancellationToken::new();
        RpcService::new(
            RpcArgs {
                rpc_listen_address: test_listen_address(),
                metrics_address: test_listen_address(),
                ..Default::default()
            },
            cancel,
        )
        .await
        .expect("Failed to create test JSON-RPC service")
    }

    #[tokio::test]
    async fn test_add_module() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        struct FooImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_multiple_methods() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;

            #[method(name = "baz")]
            fn baz(&self) -> RpcResult<u64>;
        }

        struct FooImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }

            fn baz(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_multiple_modules() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        #[rpc(server, namespace = "test")]
        trait Bar {
            #[method(name = "baz")]
            fn baz(&self) -> RpcResult<u64>;
        }

        struct FooImpl;
        struct BarImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        impl BarServer for BarImpl {
            fn baz(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();
        rpc.add_module(BarImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_conflict() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        #[rpc(server, namespace = "test")]
        trait Bar {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        struct FooImpl;
        struct BarImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        impl BarServer for BarImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();
        assert!(rpc.add_module(BarImpl.into_rpc()).is_err(),)
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let cancel = CancellationToken::new();
        let rpc = RpcService::new(
            RpcArgs {
                rpc_listen_address: test_listen_address(),
                metrics_address: test_listen_address(),
                ..Default::default()
            },
            cancel.clone(),
        )
        .await
        .unwrap();

        let handle = rpc.run().await.unwrap();

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
                metrics_address: test_listen_address(),
                ..Default::default()
            },
            cancel.clone(),
        )
        .await
        .unwrap();

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        struct FooImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();

        let metrics = rpc.metrics_for_test();
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
                .with_label_values(&["test_baz"])
                .get(),
            1
        );

        assert_eq!(
            metrics
                .requests_succeeded
                .with_label_values(&["test_baz"])
                .get(),
            0
        );

        assert_eq!(
            metrics
                .requests_failed
                .with_label_values(&["test_baz", &format!("{METHOD_NOT_FOUND_CODE}")])
                .get(),
            1
        );

        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("Shutdown should not timeout")
            .expect("Shutdown should succeed");
    }
}
