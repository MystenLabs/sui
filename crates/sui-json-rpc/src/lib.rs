// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::net::SocketAddr;
use std::str::FromStr;

use hyper::header::HeaderValue;
use hyper::Method;
pub use jsonrpsee::server::ServerHandle;
use jsonrpsee::server::{AllowHosts, ServerBuilder};
use jsonrpsee::RpcModule;
use prometheus::Registry;
use tap::TapFallible;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};

use sui_open_rpc::{Module, Project};

use crate::metrics::MetricsLayer;

pub mod api;
pub mod bcs_api;
pub mod coin_api;
pub mod error;
pub mod event_api;
pub mod governance_api;
mod metrics;
pub mod read_api;
pub mod streaming_api;
pub mod threshold_bls_api;
pub mod transaction_builder_api;
pub mod transaction_execution_api;

#[cfg(test)]
#[path = "unit_tests/rpc_server_tests.rs"]
mod rpc_server_test;

pub struct JsonRpcServerBuilder {
    module: RpcModule<()>,
    rpc_doc: Project,
    registry: Registry,
}

pub fn sui_rpc_doc(version: &str) -> Project {
    Project::new(
        version,
        "Sui JSON-RPC",
        "Sui JSON-RPC API for interaction with Sui Full node.",
        "Mysten Labs",
        "https://mystenlabs.com",
        "build@mystenlabs.com",
        "Apache-2.0",
        "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE",
    )
}

impl JsonRpcServerBuilder {
    pub fn new(version: &str, prometheus_registry: &Registry) -> anyhow::Result<Self> {
        Ok(Self {
            module: RpcModule::new(()),
            rpc_doc: sui_rpc_doc(version),
            registry: prometheus_registry.clone(),
        })
    }

    pub fn register_module<T: SuiRpcModule>(&mut self, module: T) -> Result<(), anyhow::Error> {
        self.rpc_doc.add_module(T::rpc_doc_module());
        Ok(self.module.merge(module.rpc())?)
    }

    pub async fn start(
        mut self,
        listen_address: SocketAddr,
    ) -> Result<ServerHandle, anyhow::Error> {
        let acl = match env::var("ACCESS_CONTROL_ALLOW_ORIGIN") {
            Ok(value) => {
                let allow_hosts = value
                    .split(',')
                    .into_iter()
                    .map(HeaderValue::from_str)
                    .collect::<Result<Vec<_>, _>>()?;
                AllowOrigin::list(allow_hosts)
            }
            _ => AllowOrigin::any(),
        };
        info!(?acl);

        let cors = CorsLayer::new()
            // Allow `POST` when accessing the resource
            .allow_methods([Method::POST])
            // Allow requests from any origin
            .allow_origin(acl)
            .allow_headers([hyper::header::CONTENT_TYPE]);

        self.module
            .register_method("rpc.discover", move |_, _| Ok(self.rpc_doc.clone()))?;
        let methods_names = self.module.method_names().collect::<Vec<_>>();

        let max_connection = env::var("RPC_MAX_CONNECTION")
            .ok()
            .and_then(|o| {
                u32::from_str(&o)
                    .tap_err(|e| warn!("Cannot parse RPC_MAX_CONNECTION to u32: {e}"))
                    .ok()
            })
            .unwrap_or(u32::MAX);

        let metrics_layer = MetricsLayer::new(&self.registry, &methods_names);
        let middleware = tower::ServiceBuilder::new()
            .layer(cors)
            .layer(metrics_layer);

        let server = ServerBuilder::default()
            .max_connections(max_connection)
            .set_host_filtering(AllowHosts::Any)
            .set_middleware(middleware)
            .build(listen_address)
            .await?;
        let addr = server.local_addr()?;
        let handle = server.start(self.module)?;

        info!(local_addr =? addr, "Sui JSON-RPC server listening on {addr}");
        info!("Available JSON-RPC methods : {:?}", methods_names);

        Ok(handle)
    }
}

pub trait SuiRpcModule
where
    Self: Sized,
{
    fn rpc(self) -> RpcModule<Self>;
    fn rpc_doc_module() -> Module;
}
