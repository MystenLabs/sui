// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http;
use hyper::header::HeaderName;
use hyper::header::HeaderValue;
use hyper::Method;
use hyper::Request;
use jsonrpsee::RpcModule;
use metrics::Metrics;
use metrics::MetricsLayer;
use prometheus::Registry;
use sui_core::traffic_controller::metrics::TrafficControllerMetrics;
use sui_core::traffic_controller::TrafficController;
use sui_types::traffic_control::PolicyConfig;
use sui_types::traffic_control::RemoteFirewallConfig;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

pub use balance_changes::*;
pub use object_changes::*;
pub use sui_config::node::ServerType;
use sui_json_rpc_api::{
    CLIENT_REQUEST_METHOD_HEADER, CLIENT_SDK_TYPE_HEADER, CLIENT_SDK_VERSION_HEADER,
    CLIENT_TARGET_API_VERSION_HEADER,
};
use sui_open_rpc::{Module, Project};
use traffic_control::TrafficControllerService;

use crate::error::Error;

pub mod authority_state;
mod balance_changes;
pub mod bridge_api;
pub mod coin_api;
pub mod error;
pub mod governance_api;
pub mod indexer_api;
pub mod logger;
mod metrics;
pub mod move_utils;
mod object_changes;
pub mod read_api;
mod traffic_control;
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
                HeaderName::from_static(CLIENT_REQUEST_METHOD_HEADER),
            ]);
        Ok(cors)
    }

    fn trace_layer() -> TraceLayer<
        tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
        impl tower_http::trace::MakeSpan<Body> + Clone,
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

                let origin = request
                    .headers()
                    .get("origin")
                    .and_then(|v| v.to_str().ok())
                    .map(tracing::field::display);

                tracing::info_span!(
                    "json-rpc-request",
                    "x-req-id" = request_id,
                    "origin" = origin
                )
            })
            .on_request(())
            .on_response(())
            .on_body_chunk(())
            .on_eos(())
            .on_failure(())
    }

    pub async fn to_router(&self, server_type: ServerType) -> Result<axum::Router, Error> {
        let rpc_docs = self.rpc_doc.clone();
        let mut module = self.module.clone();
        module.register_method("rpc.discover", move |_, _, _| {
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(rpc_docs.clone())
        })?;
        let methods_names = module.method_names().collect::<Vec<_>>();

        let metrics = Arc::new(Metrics::new(&self.registry, &methods_names));
        let traffic_controller_metrics = TrafficControllerMetrics::new(&self.registry);
        let traffic_controller = self.policy_config.clone().map(|policy| {
            Arc::new(TrafficController::init(
                policy,
                traffic_controller_metrics,
                self.firewall_config.clone(),
            ))
        });
        let client_id_source = self
            .policy_config
            .clone()
            .map(|policy| policy.client_id_source);

        let metrics_clone = metrics.clone();
        let middleware = ServiceBuilder::new()
            .layer(Self::trace_layer())
            .layer(Self::cors()?)
            .map_request(move |mut request: http::Request<_>| {
                metrics_clone.on_http_request(request.headers());
                if let Some(client_id_source) = client_id_source.clone() {
                    traffic_control::determine_client_ip(client_id_source, &mut request);
                }
                request
            });

        let (stop_handle, server_handle) = jsonrpsee::server::stop_channel();
        std::mem::forget(server_handle);

        let rpc_middleware = jsonrpsee::server::middleware::rpc::RpcServiceBuilder::new()
            .layer_fn(move |s| MetricsLayer::new(s, metrics.clone()))
            .layer_fn(move |s| TrafficControllerService::new(s, traffic_controller.clone()));
        let service_builder = jsonrpsee::server::ServerBuilder::new()
            // Since we're not using jsonrpsee's server to actually handle connections this value
            // is instead limiting the number of concurrent requests and has no impact on the
            // number of connections. As such, for now we can just set this to a very high value to
            // disable it artificially limiting us to ~100 conncurrent requests.
            .max_connections(u32::MAX)
            // Before we updated jsonrpsee, batches were disabled so lets keep them disabled.
            .set_batch_request_config(jsonrpsee::server::BatchRequestConfig::Disabled)
            // We don't limit response body sizes.
            .max_response_body_size(u32::MAX)
            .set_rpc_middleware(rpc_middleware);

        let mut router = axum::Router::new();
        match server_type {
            ServerType::WebSocket => {
                let service = JsonRpcService(
                    service_builder
                        .ws_only()
                        .to_service_builder()
                        .build(module, stop_handle),
                );
                router = router
                    .route("/", axum::routing::get_service(service.clone()))
                    .route("/subscribe", axum::routing::get_service(service));
            }
            ServerType::Http => {
                let service = JsonRpcService(
                    service_builder
                        .http_only()
                        .to_service_builder()
                        .build(module, stop_handle),
                );
                router = router
                    .route("/", axum::routing::post_service(service.clone()))
                    .route("/json-rpc", axum::routing::post_service(service.clone()))
                    .route("/public", axum::routing::post_service(service));
            }
            ServerType::Both => {
                let service = JsonRpcService(
                    service_builder
                        .to_service_builder()
                        .build(module, stop_handle),
                );
                router = router
                    .route("/", axum::routing::post_service(service.clone()))
                    .route("/", axum::routing::get_service(service.clone()))
                    .route("/subscribe", axum::routing::get_service(service.clone()))
                    .route("/json-rpc", axum::routing::post_service(service.clone()))
                    .route("/public", axum::routing::post_service(service));
            }
        }

        let app = router.layer(middleware);

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

        let listener = tokio::net::TcpListener::bind(&listen_address)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
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

use jsonrpsee::core::BoxError;

#[derive(Clone)]
struct JsonRpcService<S>(S);

impl<S, RequestBody> tower::Service<http::Request<RequestBody>> for JsonRpcService<S>
where
    S: tower::Service<
        http::Request<RequestBody>,
        Error = BoxError,
        Response = http::Response<jsonrpsee::server::HttpBody>,
        Future: Send + 'static,
    >,
{
    type Response = http::Response<jsonrpsee::server::HttpBody>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<RequestBody>) -> Self::Future {
        let fut = self.0.call(request);
        Box::pin(async move {
            match fut.await {
                Ok(response) => Ok(response),
                Err(e) => Ok(http::Response::builder()
                    .status(http::status::StatusCode::INTERNAL_SERVER_ERROR)
                    .body(jsonrpsee::server::HttpBody::from(e.to_string()))
                    .unwrap()),
            }
        })
    }
}
