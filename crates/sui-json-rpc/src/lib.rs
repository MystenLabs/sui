// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::net::SocketAddr;
use std::str::FromStr;

use hyper::header::HeaderName;
use hyper::header::HeaderValue;
use hyper::Method;
use jsonrpsee::server::{AllowHosts, ServerBuilder};
use jsonrpsee::RpcModule;
use prometheus::Registry;
use tap::TapFallible;
use tokio::runtime::Handle;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};

pub use balance_changes::*;
pub use object_changes::*;
use sui_open_rpc::{Module, Project};

use crate::error::Error;
use crate::metrics::MetricsLogger;
use crate::routing_layer::RoutingLayer;

pub mod api;
mod balance_changes;
pub mod coin_api;
pub mod error;
pub mod governance_api;
pub mod indexer_api;
pub mod logger;
mod metrics;
pub mod move_utils;
mod object_changes;
pub mod read_api;
mod routing_layer;
pub mod transaction_builder_api;
pub mod transaction_execution_api;

pub const CLIENT_SDK_TYPE_HEADER: &str = "client-sdk-type";
/// The version number of the SDK itself. This can be different from the API version.
pub const CLIENT_SDK_VERSION_HEADER: &str = "client-sdk-version";
/// The RPC API version that the client is targeting. Different SDK versions may target the same
/// API version.
pub const CLIENT_TARGET_API_VERSION_HEADER: &str = "client-target-api-version";
pub const APP_NAME_HEADER: &str = "app-name";

pub const MAX_REQUEST_SIZE: u32 = 2 << 30;

#[cfg(test)]
#[path = "unit_tests/rpc_server_tests.rs"]
mod rpc_server_test;
#[cfg(test)]
#[path = "unit_tests/transaction_tests.rs"]
mod transaction_tests;

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
    pub fn new(version: &str, prometheus_registry: &Registry) -> Self {
        Self {
            module: RpcModule::new(()),
            rpc_doc: sui_rpc_doc(version),
            registry: prometheus_registry.clone(),
        }
    }

    pub fn register_module<T: SuiRpcModule>(&mut self, module: T) -> Result<(), Error> {
        self.rpc_doc.add_module(T::rpc_doc_module());
        Ok(self.module.merge(module.rpc())?)
    }

    pub async fn start(
        mut self,
        listen_address: SocketAddr,
        custom_runtime: Option<Handle>,
    ) -> Result<ServerHandle, Error> {
        let acl = match env::var("ACCESS_CONTROL_ALLOW_ORIGIN") {
            Ok(value) => {
                let allow_hosts = value
                    .split(',')
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
            .allow_headers([
                hyper::header::CONTENT_TYPE,
                HeaderName::from_static(CLIENT_SDK_TYPE_HEADER),
                HeaderName::from_static(CLIENT_SDK_VERSION_HEADER),
                HeaderName::from_static(CLIENT_TARGET_API_VERSION_HEADER),
                HeaderName::from_static(APP_NAME_HEADER),
            ]);

        let routing = self.rpc_doc.method_routing.clone();

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

        let metrics_logger = MetricsLogger::new(&self.registry, &methods_names);

        let disable_routing = env::var("DISABLE_BACKWARD_COMPATIBILITY")
            .ok()
            .and_then(|v| bool::from_str(&v).ok())
            .unwrap_or_default();
        info!(
            "Compatibility method routing {}.",
            if disable_routing {
                "disabled"
            } else {
                "enabled"
            }
        );
        // We need to use the routing layer to block access to the old methods when routing is disabled.
        let routing_layer = RoutingLayer::new(routing, disable_routing);

        let middleware = tower::ServiceBuilder::new()
            .layer(cors)
            .layer(routing_layer);

        let mut builder = ServerBuilder::default()
            .batch_requests_supported(false)
            .max_response_body_size(MAX_REQUEST_SIZE)
            .max_connections(max_connection)
            .set_host_filtering(AllowHosts::Any)
            .set_middleware(middleware)
            .set_logger(metrics_logger);

        if let Some(custom_runtime) = custom_runtime {
            builder = builder.custom_tokio_runtime(custom_runtime);
        }

        let server = builder.build(listen_address).await?;

        let addr = server.local_addr()?;
        let handle = ServerHandle {
            handle: server.start(self.module)?,
        };
        info!(local_addr =? addr, "Sui JSON-RPC server listening on {addr}");
        info!("Available JSON-RPC methods : {:?}", methods_names);
        Ok(handle)
    }
}

pub struct ServerHandle {
    handle: jsonrpsee::server::ServerHandle,
}

impl ServerHandle {
    pub async fn stopped(self) {
        self.handle.stopped().await
    }
}

pub trait SuiRpcModule
where
    Self: Sized,
{
    fn rpc(self) -> RpcModule<Self>;
    fn rpc_doc_module() -> Module;
}
