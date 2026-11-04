// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The kvstore gRPC server.
//!
//! It hosts `grpc.health.v1` and gRPC reflection, and further services, such as
//! [`watermark_service::WatermarkService`], are registered through [`RpcService::add_service`].
//! The overall server status (the "" service name) and every registered service report
//! NOT_SERVING until the readiness future passed to [`RpcService::run`] resolves, and SERVING
//! from then on. Readiness gates the instance once, after startup: the status latches, so lag that
//! develops later does not withdraw it.

pub mod proto;
pub mod watermark_service;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sui_futures::service::Service;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tonic::body::Body;
use tonic::codegen::Service as TowerService;
use tonic::server::NamedService;
use tonic::service::Routes;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;
use tonic_health::ServingStatus;
use tracing::info;

/// Service name the gRPC health protocol uses for the overall health of the server.
const OVERALL_HEALTH_SERVICE_NAME: &str = "";

/// Time between HTTP/2 pings the server sends each client. A client that disappears without
/// closing its connection (for example when its node fails) stops acknowledging them.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// How long to wait for a client to acknowledge a ping before closing its connection. Graceful
/// shutdown waits for every connection to close, so it would otherwise wait on such a client
/// indefinitely.
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(clap::Args, Clone, Debug, Default)]
pub struct RpcArgs {
    /// Address to serve the kvstore gRPC API on: health checks that report ready once this
    /// instance has caught up, and a stream of its durable watermarks. When unset, no server runs
    /// and readiness is not tracked.
    #[arg(long)]
    pub rpc_listen_address: Option<SocketAddr>,
}

/// Sets up the kvstore gRPC server: registered services, reflection, and health checks.
pub struct RpcService {
    /// Address to accept incoming gRPC connections on.
    rpc_listen_address: SocketAddr,

    /// Routes for every registered service, excluding the internal health and reflection
    /// services, which are added in [`RpcService::run`].
    routes: Routes,

    /// File descriptor sets are added to these builders to be exposed via the reflection service.
    reflection_v1: tonic_reflection::server::Builder<'static>,
    reflection_v1alpha: tonic_reflection::server::Builder<'static>,

    /// Names of the registered (non-internal) services, whose health status follows readiness.
    service_names: Vec<&'static str>,

    /// Cancelled when the server starts its graceful shutdown.
    shutdown: CancellationToken,
}

impl RpcService {
    pub fn new(rpc_listen_address: SocketAddr) -> Self {
        Self {
            rpc_listen_address,
            routes: Routes::default(),
            reflection_v1: tonic_reflection::server::Builder::configure(),
            reflection_v1alpha: tonic_reflection::server::Builder::configure(),
            service_names: vec![],
            shutdown: CancellationToken::new(),
        }
    }

    /// A token that is cancelled when the server starts its graceful shutdown. Graceful shutdown
    /// waits for every open request to finish, so a service with long-lived response streams must
    /// end them when this token is cancelled.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Register a file descriptor set to be exposed via the reflection service.
    pub fn register_encoded_file_descriptor_set(mut self, fds: &'static [u8]) -> Self {
        self.reflection_v1 = self.reflection_v1.register_encoded_file_descriptor_set(fds);
        self.reflection_v1alpha = self
            .reflection_v1alpha
            .register_encoded_file_descriptor_set(fds);
        self
    }

    /// Register a gRPC service. It reports NOT_SERVING until the server is ready.
    pub fn add_service<S>(mut self, service: S) -> Self
    where
        S: NamedService,
        S: TowerService<http::Request<Body>, Response = http::Response<Body>, Error = Infallible>,
        S: Clone + Send + Sync + 'static,
        S::Future: Send + 'static,
    {
        self.service_names.push(S::NAME);
        self.routes = self.routes.add_service(service);
        self
    }

    /// Bind the listener and start serving. The overall server status and every registered
    /// service switch from NOT_SERVING to SERVING once `ready` resolves.
    ///
    /// Graceful shutdown of the returned service stops the server and abandons `ready` if it has
    /// not resolved yet.
    pub async fn run(
        self,
        ready: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<Service> {
        let Self {
            rpc_listen_address,
            routes,
            reflection_v1,
            reflection_v1alpha,
            service_names,
            shutdown,
        } = self;

        let reflection_v1 = reflection_v1
            .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
            .build_v1()
            .context("Failed to build gRPC reflection v1 service")?;

        let reflection_v1alpha = reflection_v1alpha
            .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
            .build_v1alpha()
            .context("Failed to build gRPC reflection v1alpha service")?;

        let (health_reporter, health_service) = tonic_health::server::health_reporter();

        let internal_services = [
            service_name(&reflection_v1),
            service_name(&reflection_v1alpha),
            service_name(&health_service),
        ];

        let routes = routes
            .add_service(reflection_v1)
            .add_service(reflection_v1alpha)
            .add_service(health_service);

        for name in internal_services {
            health_reporter
                .set_service_status(name, ServingStatus::Serving)
                .await;
        }

        // The overall server status and every registered service wait on readiness.
        let readiness_gated: Vec<&'static str> = std::iter::once(OVERALL_HEALTH_SERVICE_NAME)
            .chain(service_names)
            .collect();

        for &name in &readiness_gated {
            health_reporter
                .set_service_status(name, ServingStatus::NotServing)
                .await;
        }

        // Graceful shutdown waits for every open request, and a `Health/Watch` stream only ends
        // once its service's status is cleared. The lock orders the readiness task's statuses
        // against shutdown clearing them, so a status is never set again after it is cleared.
        let health_service_names: Vec<&'static str> = internal_services
            .into_iter()
            .chain(readiness_gated.iter().copied())
            .collect();
        let health_reporter = Arc::new(Mutex::new(health_reporter));
        let shutdown_health_reporter = health_reporter.clone();
        let readiness_shutdown = shutdown.clone();

        // `serve_with_incoming_shutdown` ignores the builder's TCP settings, so they are set here.
        let incoming = TcpIncoming::bind(rpc_listen_address)
            .with_context(|| format!("Failed to bind kvstore gRPC server to {rpc_listen_address}"))?
            .with_nodelay(Some(true));

        info!("Starting kvstore gRPC server on {rpc_listen_address}");

        // Aborted on shutdown, so a catch-up that is still in progress does not hold up the
        // graceful shutdown of the server.
        let readiness = Service::new().spawn_aborting(async move {
            ready.await;
            let health_reporter = health_reporter.lock().await;
            if readiness_shutdown.is_cancelled() {
                return Ok(());
            }
            for name in readiness_gated {
                health_reporter
                    .set_service_status(name, ServingStatus::Serving)
                    .await;
            }
            info!("kvstore gRPC server is now SERVING");
            Ok(())
        });

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let service = Service::new()
            .with_shutdown_signal(async move {
                shutdown.cancel();
                let mut health_reporter = shutdown_health_reporter.lock().await;
                for name in health_service_names {
                    health_reporter.clear_service_status(name).await;
                }
                drop(health_reporter);
                let _ = shutdown_tx.send(());
            })
            .spawn(async move {
                Server::builder()
                    .http2_keepalive_interval(Some(KEEPALIVE_INTERVAL))
                    .http2_keepalive_timeout(Some(KEEPALIVE_TIMEOUT))
                    .add_routes(routes)
                    .serve_with_incoming_shutdown(incoming, async move {
                        let _ = shutdown_rx.await;
                    })
                    .await
                    .context("kvstore gRPC server failed")
            })
            .attach(readiness);

        Ok(service)
    }
}

fn service_name<S: NamedService>(_: &S) -> &'static str {
    S::NAME
}

#[cfg(test)]
mod tests {
    use std::future::Ready;
    use std::task::Context as TaskContext;
    use std::task::Poll;
    use std::time::Duration;

    use tonic::transport::Channel;
    use tonic_health::pb::HealthCheckRequest;
    use tonic_health::pb::health_check_response::ServingStatus as ProtoServingStatus;
    use tonic_health::pb::health_client::HealthClient;

    use crate::testing::get_available_port;

    use super::*;

    /// Service name of the `grpc.health.v1` health service.
    const HEALTH_SERVICE_NAME: &str = "grpc.health.v1.Health";

    /// A registered service whose health status is gated on readiness. Every request gets an empty
    /// response.
    #[derive(Clone)]
    struct GatedService;

    impl NamedService for GatedService {
        const NAME: &'static str = "sui.kvstore.test.Gated";
    }

    impl TowerService<http::Request<Body>> for GatedService {
        type Response = http::Response<Body>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: http::Request<Body>) -> Self::Future {
            std::future::ready(Ok(http::Response::new(Body::default())))
        }
    }

    async fn connect(rpc_listen_address: SocketAddr) -> HealthClient<Channel> {
        let channel = Channel::from_shared(format!("http://{rpc_listen_address}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        HealthClient::new(channel)
    }

    async fn service_status(
        client: &mut HealthClient<Channel>,
        service: &str,
    ) -> ProtoServingStatus {
        client
            .check(HealthCheckRequest {
                service: service.to_string(),
            })
            .await
            .unwrap()
            .into_inner()
            .status()
    }

    /// Poll the status of `service` until it reports SERVING, or give up after 5 seconds and
    /// return the last status seen.
    async fn poll_until_serving(
        client: &mut HealthClient<Channel>,
        service: &str,
    ) -> ProtoServingStatus {
        let mut status = service_status(client, service).await;
        for _ in 0..50 {
            if status == ProtoServingStatus::Serving {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            status = service_status(client, service).await;
        }
        status
    }

    #[tokio::test]
    async fn test_overall_status_follows_readiness() {
        let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], get_available_port()));
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        let service = RpcService::new(rpc_listen_address)
            .run(async move {
                let _ = ready_rx.await;
            })
            .await
            .unwrap();

        let mut client = connect(rpc_listen_address).await;

        assert_eq!(
            service_status(&mut client, OVERALL_HEALTH_SERVICE_NAME).await,
            ProtoServingStatus::NotServing
        );

        ready_tx.send(()).unwrap();

        assert_eq!(
            poll_until_serving(&mut client, OVERALL_HEALTH_SERVICE_NAME).await,
            ProtoServingStatus::Serving
        );

        drop(client);
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_registered_service_follows_readiness() {
        let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], get_available_port()));
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        let service = RpcService::new(rpc_listen_address)
            .add_service(GatedService)
            .run(async move {
                let _ = ready_rx.await;
            })
            .await
            .unwrap();

        let mut client = connect(rpc_listen_address).await;

        assert_eq!(
            service_status(&mut client, HEALTH_SERVICE_NAME).await,
            ProtoServingStatus::Serving
        );
        assert_eq!(
            service_status(&mut client, GatedService::NAME).await,
            ProtoServingStatus::NotServing
        );

        ready_tx.send(()).unwrap();

        assert_eq!(
            poll_until_serving(&mut client, GatedService::NAME).await,
            ProtoServingStatus::Serving
        );
        assert_eq!(
            service_status(&mut client, HEALTH_SERVICE_NAME).await,
            ProtoServingStatus::Serving
        );

        drop(client);
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_while_not_ready() {
        let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], get_available_port()));

        let service = RpcService::new(rpc_listen_address)
            .run(std::future::pending())
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(5), service.shutdown())
            .await
            .expect("shutdown waited on the unresolved readiness future")
            .unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_ends_open_health_watches() {
        let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], get_available_port()));

        let service = RpcService::new(rpc_listen_address)
            .run(std::future::pending())
            .await
            .unwrap();

        let mut client = connect(rpc_listen_address).await;
        let mut watch = client
            .watch(HealthCheckRequest {
                service: OVERALL_HEALTH_SERVICE_NAME.to_string(),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            watch.message().await.unwrap().unwrap().status(),
            ProtoServingStatus::NotServing
        );

        tokio::time::timeout(Duration::from_secs(5), service.shutdown())
            .await
            .expect("shutdown waited on an open health watch")
            .unwrap();
    }

    #[tokio::test]
    async fn test_bind_failure() {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_listen_address = occupied.local_addr().unwrap();

        let result = RpcService::new(rpc_listen_address)
            .run(std::future::pending())
            .await;

        let error = format!("{:#}", result.unwrap_err());
        assert!(
            error.contains(&rpc_listen_address.to_string()),
            "bind error should name the address: {error}"
        );
    }
}
