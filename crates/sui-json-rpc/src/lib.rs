// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::net::SocketAddr;
use std::str::FromStr;

use hyper::header::HeaderName;
use hyper::header::HeaderValue;
use hyper::Body;
use hyper::Method;
use hyper::Request;
use jsonrpsee::RpcModule;
use prometheus::Registry;
use sui_core::traffic_controller::metrics::TrafficControllerMetrics;
use sui_types::traffic_control::PolicyConfig;
use sui_types::traffic_control::RemoteFirewallConfig;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

pub use balance_changes::*;
pub use object_changes::*;
pub use sui_config::node::ServerType;
use sui_json_rpc_api::{
    CLIENT_SDK_TYPE_HEADER, CLIENT_SDK_VERSION_HEADER, CLIENT_TARGET_API_VERSION_HEADER,
};
use sui_open_rpc::{Module, Project};

use crate::error::Error;
use crate::metrics::MetricsLogger;
use crate::routing_layer::RpcRouter;

pub mod authority_state;
pub mod axum_router;
mod balance_changes;
pub mod bridge_api;
pub mod coin_api;
pub mod error;
pub mod governance_api;
pub mod indexer_api;
pub mod logger;
mod metrics;
pub mod move_utils;
pub mod name_service;
mod object_changes;
pub mod read_api;
mod routing_layer;
pub mod transaction_builder_api;
pub mod transaction_execution_api;

pub const APP_NAME_HEADER: &str = "app-name";

pub const MAX_REQUEST_SIZE: u32 = 2 << 30;

pub struct JsonRpcServerBuilder {
    module: RpcModule<()>,
    rpc_doc: Project,
    registry: Registry,
    policy_config: Option<PolicyConfig>,
    firewall_config: Option<RemoteFirewallConfig>,
}

pub fn sui_rpc_doc(version: &str) -> Project {
    Project::new(
        version,
        "Sui JSON-RPC",
        "Sui JSON-RPC API for interaction with Sui Full node. Make RPC calls using https://fullnode.NETWORK.sui.io:443, where NETWORK is the network you want to use (testnet, devnet, mainnet). By default, local networks use port 9000.",
        "Mysten Labs",
        "https://mystenlabs.com",
        "build@mystenlabs.com",
        "Apache-2.0",
        "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE",
    )
}

impl JsonRpcServerBuilder {
    pub fn new(
        version: &str,
        prometheus_registry: &Registry,
        policy_config: Option<PolicyConfig>,
        firewall_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        Self {
            module: RpcModule::new(()),
            rpc_doc: sui_rpc_doc(version),
            registry: prometheus_registry.clone(),
            policy_config,
            firewall_config,
        }
    }

    pub fn register_module<T: SuiRpcModule>(&mut self, module: T) -> Result<(), Error> {
        self.rpc_doc.add_module(T::rpc_doc_module());
        Ok(self.module.merge(module.rpc())?)
    }

    fn cors() -> Result<CorsLayer, Error> {
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
        Ok(cors)
    }

    fn trace_layer() -> TraceLayer<
        tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
        impl tower_http::trace::MakeSpan<hyper::Body> + Clone,
        (),
        (),
        (),
        (),
        (),
    > {
        TraceLayer::new_for_http()
            .make_span_with(|request: &Request<Body>| {
                let request_id = request
                    .headers()
                    .get("x-req-id")
                    .and_then(|v| v.to_str().ok())
                    .map(tracing::field::display);

                tracing::info_span!("json-rpc-request", "x-req-id" = request_id)
            })
            .on_request(())
            .on_response(())
            .on_body_chunk(())
            .on_eos(())
            .on_failure(())
    }

    pub async fn to_router(&self, server_type: ServerType) -> Result<axum::Router, Error> {
        let routing = self.rpc_doc.method_routing.clone();

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
        let rpc_router = RpcRouter::new(routing, disable_routing);

        let rpc_docs = self.rpc_doc.clone();
        let mut module = self.module.clone();
        module.register_method("rpc.discover", move |_, _| Ok(rpc_docs.clone()))?;
        let methods_names = module.method_names().collect::<Vec<_>>();

        let metrics_logger = MetricsLogger::new(&self.registry, &methods_names);
        let traffic_controller_metrics = TrafficControllerMetrics::new(&self.registry);

        let middleware = tower::ServiceBuilder::new()
            .layer(Self::trace_layer())
            .layer(Self::cors()?);

        let service = crate::axum_router::JsonRpcService::new(
            module.into(),
            rpc_router,
            metrics_logger,
            self.firewall_config.clone(),
            self.policy_config.clone(),
            traffic_controller_metrics,
        );

        let mut router = axum::Router::new();

        match server_type {
            ServerType::WebSocket => {
                router = router
                    .route(
                        "/",
                        axum::routing::get(crate::axum_router::ws::ws_json_rpc_upgrade),
                    )
                    .route(
                        "/subscribe",
                        axum::routing::get(crate::axum_router::ws::ws_json_rpc_upgrade),
                    );
            }
            ServerType::Http => {
                router = router
                    .route(
                        "/",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    )
                    .route(
                        "/json-rpc",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    )
                    .route(
                        "/public",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    );
            }
            ServerType::Both => {
                router = router
                    .route(
                        "/",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    )
                    .route(
                        "/",
                        axum::routing::get(crate::axum_router::ws::ws_json_rpc_upgrade),
                    )
                    .route(
                        "/subscribe",
                        axum::routing::get(crate::axum_router::ws::ws_json_rpc_upgrade),
                    )
                    .route(
                        "/json-rpc",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    )
                    .route(
                        "/public",
                        axum::routing::post(crate::axum_router::json_rpc_handler),
                    );
            }
        }

        let app = router.with_state(service).layer(middleware);

        info!("Available JSON-RPC methods : {:?}", methods_names);

        Ok(app)
    }

    pub async fn start(
        self,
        listen_address: SocketAddr,
        _custom_runtime: Option<Handle>,
        server_type: ServerType,
        cancel: Option<CancellationToken>,
    ) -> Result<ServerHandle, Error> {
        let app = self.to_router(server_type).await?;

        let server = axum::Server::bind(&listen_address)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>());

        let addr = server.local_addr();

        let handle = tokio::spawn(async move {
            server.await.unwrap();
            if let Some(cancel) = cancel {
                // Signal that the server is shutting down, so other tasks can clean-up.
                cancel.cancel();
            }
        });

        let handle = ServerHandle {
            handle: ServerHandleInner::Axum(handle),
        };
        info!(local_addr =? addr, "Sui JSON-RPC server listening on {addr}");
        Ok(handle)
    }
}

pub struct ServerHandle {
    handle: ServerHandleInner,
}

impl ServerHandle {
    pub async fn stopped(self) {
        match self.handle {
            ServerHandleInner::Axum(handle) => handle.await.unwrap(),
        }
    }
}

enum ServerHandleInner {
    Axum(tokio::task::JoinHandle<()>),
}

pub trait SuiRpcModule
where
    Self: Sized,
{
    fn rpc(self) -> RpcModule<Self>;
    fn rpc_doc_module() -> Module;
}
