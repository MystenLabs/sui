// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::PathBuf;
use std::{convert::Infallible, sync::Arc};

use anyhow::Context;
use axum::extract::Request;
use axum::response::IntoResponse;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use futures::future::OptionFuture;
use metrics::RpcMetrics;
use middleware::metrics::MakeMetricsHandler;
use middleware::version::Version;
use mysten_network::callback::CallbackLayer;
use prometheus::Registry;
use tokio::join;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::server::NamedService;
use tonic_health::ServingStatus;
use tower::Service;
use tracing::{error, info};

pub(crate) mod consistent_service;
mod error;
mod metrics;
mod middleware;
pub(crate) mod pagination;
pub(crate) mod state;
mod type_filter;

#[derive(clap::Args, Clone, Debug)]
pub struct RpcArgs {
    /// Address to accept incoming RPC connections on.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,

    /// TLS configuration
    #[clap(flatten)]
    pub tls: TlsArgs,
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct TlsArgs {
    /// Address to accept incoming TLS/HTTPS connections on
    #[clap(long, requires_all = &["tls_cert", "tls_key"])]
    pub rpc_tls_listen_address: Option<SocketAddr>,

    /// Path to TLS certificate file (PEM format)
    #[clap(long, requires_all = &["rpc_tls_listen_address", "tls_key"])]
    pub tls_cert: Option<PathBuf>,

    /// Path to TLS private key file (PEM format)
    #[clap(long, requires_all = &["rpc_tls_listen_address", "tls_cert"])]
    pub tls_key: Option<PathBuf>,
}

/// Responsible for the set-up of a gRPC service -- adding services, configuring reflection,
/// health-checks, logging and metrics middleware, etc.
pub(crate) struct RpcService<'d> {
    /// Address to accept incoming RPC connections on.
    rpc_listen_address: SocketAddr,

    /// Optional address to accept incoming TLS RPC connections on.
    rpc_tls_listen_address: Option<SocketAddr>,

    /// TLS configuration
    tls_config: Option<RustlsConfig>,

    /// The version string to report with each response, as an HTTP header.
    version: &'static str,

    /// File descriptors are added to these builders to eventually be exposed via the reflection
    /// service.
    reflection_v1: tonic_reflection::server::Builder<'d>,
    reflection_v1alpha: tonic_reflection::server::Builder<'d>,

    /// The names of gRPC services registered with this instance.
    service_names: Vec<&'static str>,

    /// The axum router that wil handle incoming requests.
    router: Router,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,

    /// Cancellation token controls lifecycle for all RPC-related services.
    cancel: CancellationToken,
}

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

impl<'d> RpcService<'d> {
    pub(crate) async fn new(
        args: RpcArgs,
        version: &'static str,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let RpcArgs {
            rpc_listen_address,
            tls,
        } = args;

        let TlsArgs {
            rpc_tls_listen_address,
            tls_cert,
            tls_key,
        } = tls;

        let tls_config = if let (Some(cert), Some(key)) = (tls_cert, tls_key) {
            Some(
                RustlsConfig::from_pem_file(cert, key)
                    .await
                    .context("Failed to load TLS configuration")?,
            )
        } else {
            None
        };

        Ok(Self {
            rpc_listen_address,
            rpc_tls_listen_address,
            tls_config,
            version,
            reflection_v1: tonic_reflection::server::Builder::configure(),
            reflection_v1alpha: tonic_reflection::server::Builder::configure(),
            service_names: vec![],
            router: Router::new(),
            metrics: Arc::new(RpcMetrics::new(registry)),
            cancel,
        })
    }

    /// Register a file descriptor set to be exposed via the reflection service.
    pub(crate) fn register_encoded_file_descriptor_set(mut self, fds: &'d [u8]) -> Self {
        self.reflection_v1 = self.reflection_v1.register_encoded_file_descriptor_set(fds);
        self.reflection_v1alpha = self
            .reflection_v1alpha
            .register_encoded_file_descriptor_set(fds);
        self
    }

    /// Register a new gRPC service.
    pub(crate) fn add_service<S>(mut self, s: S) -> Self
    where
        S: Clone + Send + Sync + 'static,
        S: NamedService,
        S: Service<Request, Response: IntoResponse, Error = Infallible>,
        S::Future: Send + 'static,
        S::Error: Send + Into<BoxError>,
    {
        self.service_names.push(S::NAME);
        self.router = add_service(self.router, s);
        self
    }

    /// Run the RPC service. This binds the listener and exposes handlers for the RPC service.
    pub(crate) async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let Self {
            rpc_listen_address,
            rpc_tls_listen_address,
            tls_config,
            version,
            reflection_v1,
            reflection_v1alpha,
            mut service_names,
            mut router,
            metrics,
            cancel,
        } = self;

        let reflection_v1 = reflection_v1
            .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
            .build_v1()
            .unwrap();

        let reflection_v1alpha = reflection_v1alpha
            .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
            .build_v1alpha()
            .unwrap();

        let (health_reporter, health_service) = tonic_health::server::health_reporter();

        service_names.extend([
            service_name(&reflection_v1),
            service_name(&reflection_v1alpha),
            service_name(&health_service),
        ]);

        router = add_service(router, reflection_v1);
        router = add_service(router, reflection_v1alpha);
        router = add_service(router, health_service);
        router = router
            .layer(CallbackLayer::new(MakeMetricsHandler::new(metrics)))
            .layer(axum::middleware::from_fn_with_state(
                Version(version),
                middleware::version::set_version,
            ));

        for service_name in service_names {
            health_reporter
                .set_service_status(service_name, ServingStatus::Serving)
                .await;
        }

        // Start HTTPS server if TLS is configured
        let https_service: OptionFuture<_> =
            if let (Some(listen_address), Some(config)) = (rpc_tls_listen_address, tls_config) {
                info!("Starting Consistent RPC TLS service on {listen_address}");

                // Handle graceful shutdown for TLS service
                let handle = Handle::new();
                tokio::spawn({
                    let handle = handle.clone();
                    let cancel = cancel.clone();
                    async move {
                        cancel.cancelled().await;
                        handle.graceful_shutdown(None);
                    }
                });

                Some(
                    axum_server::bind_rustls(listen_address, config)
                        .handle(handle)
                        .serve(router.clone().into_make_service()),
                )
            } else {
                None
            }
            .into();

        // Start HTTP server
        info!("Starting Consistent RPC service on {rpc_listen_address}");
        let listener = TcpListener::bind(rpc_listen_address)
            .await
            .context("Failed to bind Consistent RPC to listen address")?;

        let http_service = axum::serve(listener, router.clone()).with_graceful_shutdown({
            let cancel = cancel.clone();
            async move {
                cancel.cancelled().await;
                info!("Shutting down Consistent RPC HTTP service");
            }
        });

        // Return a single task that waits for all servers
        Ok(tokio::spawn(async move {
            let (https, http) = join!(https_service, http_service);

            if let Err(e) = https.transpose() {
                error!("Failed to start Consistent RPC TLS service: {e:?}");
                cancel.cancel();
            }

            if let Err(e) = http {
                error!("Failed to start Consistent RPC service: {e:?}");
                cancel.cancel();
            }
        }))
    }
}

impl Default for RpcArgs {
    fn default() -> Self {
        Self {
            rpc_listen_address: "0.0.0.0:7001".parse().unwrap(),
            tls: TlsArgs::default(),
        }
    }
}

fn service_name<S: NamedService>(_: &S) -> &'static str {
    S::NAME
}

fn add_service<S>(router: Router, s: S) -> Router
where
    S: Clone + Send + Sync + 'static,
    S: NamedService,
    S: Service<Request, Response: IntoResponse, Error = Infallible>,
    S::Future: Send + 'static,
    S::Error: Send + Into<BoxError>,
{
    router.route_service(&format!("/{}/{{*rest}}", S::NAME), s)
}
