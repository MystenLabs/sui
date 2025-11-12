// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use std::task::{Context, Poll};
use std::time::Duration;

use eyre::{Result, eyre};
use mysten_network::{
    config::Config,
    metrics::{
        DefaultMetricsCallbackProvider, GRPC_ENDPOINT_PATH_HEADER, MetricsCallbackProvider,
        MetricsHandler,
    },
    multiaddr::{Multiaddr, Protocol},
};
use tokio_rustls::rustls::ServerConfig;
use tonic::codegen::http::HeaderValue;
use tonic::{
    body::Body,
    codegen::http::{Request, Response},
    server::NamedService,
};
use tower::{Layer, Service, ServiceBuilder, ServiceExt};
use tower_http::propagate_header::PropagateHeaderLayer;
use tower_http::set_header::SetRequestHeaderLayer;
use tower_http::trace::TraceLayer;

pub const DEFAULT_GRPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

pub struct ServerBuilder<M: MetricsCallbackProvider = DefaultMetricsCallbackProvider> {
    config: Config,
    metrics_provider: M,
    router: tonic::service::Routes,
    health_reporter: tonic_health::server::HealthReporter,
}

impl<M: MetricsCallbackProvider> ServerBuilder<M> {
    pub fn from_config(config: &Config, metrics_provider: M) -> Self {
        let (health_reporter, health_service) = tonic_health::server::health_reporter();
        let router = tonic::service::Routes::new(health_service);

        Self {
            config: config.to_owned(),
            metrics_provider,
            router,
            health_reporter,
        }
    }

    pub fn health_reporter(&self) -> tonic_health::server::HealthReporter {
        self.health_reporter.clone()
    }

    /// Add a new service to this Server.
    pub fn add_service<S>(mut self, svc: S) -> Self
    where
        S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
            + NamedService
            + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
    {
        self.router = self.router.add_service(svc);
        self
    }

    pub async fn bind(self, addr: &Multiaddr, tls_config: Option<ServerConfig>) -> Result<Server> {
        let http_config = self
            .config
            .http_config()
            // Temporarily continue allowing clients to connection without TLS even when the server
            // is configured with a tls_config
            .allow_insecure(true);

        let request_timeout = self
            .config
            .request_timeout
            .unwrap_or(DEFAULT_GRPC_REQUEST_TIMEOUT);
        let metrics_provider = self.metrics_provider;
        let metrics = MetricsHandler::new(metrics_provider.clone());
        let request_metrics = TraceLayer::new_for_grpc()
            .on_request(metrics.clone())
            .on_response(metrics.clone())
            .on_failure(metrics);

        fn add_path_to_request_header<T>(request: &Request<T>) -> Option<HeaderValue> {
            let path = request.uri().path();
            Some(HeaderValue::from_str(path).unwrap())
        }

        let limiting_layers = ServiceBuilder::new()
            .option_layer(
                self.config
                    .load_shed
                    .unwrap_or_default()
                    .then_some(tower::load_shed::LoadShedLayer::new()),
            )
            .option_layer(
                self.config
                    .global_concurrency_limit
                    .map(tower::limit::GlobalConcurrencyLimitLayer::new),
            );
        let route_layers = ServiceBuilder::new()
            .map_request(|mut request: http::Request<_>| {
                if let Some(connect_info) = request.extensions().get::<sui_http::ConnectInfo>() {
                    let tonic_connect_info = tonic::transport::server::TcpConnectInfo {
                        local_addr: Some(connect_info.local_addr),
                        remote_addr: Some(connect_info.remote_addr),
                    };
                    request.extensions_mut().insert(tonic_connect_info);
                }
                request
            })
            .layer(RequestLifetimeLayer { metrics_provider })
            .layer(SetRequestHeaderLayer::overriding(
                GRPC_ENDPOINT_PATH_HEADER.clone(),
                add_path_to_request_header,
            ))
            .layer(request_metrics)
            .layer(PropagateHeaderLayer::new(GRPC_ENDPOINT_PATH_HEADER.clone()))
            .layer_fn(move |service| {
                mysten_network::grpc_timeout::GrpcTimeout::new(service, request_timeout)
            });

        let mut builder = sui_http::Builder::new().config(http_config);

        if let Some(tls_config) = tls_config {
            builder = builder.tls_config(tls_config);
        }

        let server_handle = builder
            .serve(
                addr,
                limiting_layers.service(
                    self.router
                        .into_axum_router()
                        .layer(route_layers)
                        .into_service()
                        .map_err(tower::BoxError::from),
                ),
            )
            .map_err(|e| eyre!(e))?;

        let local_addr = update_tcp_port_in_multiaddr(addr, server_handle.local_addr().port());
        Ok(Server {
            server: server_handle,
            local_addr,
            health_reporter: self.health_reporter,
        })
    }
}

/// TLS server name to use for the public Sui validator interface.
pub const SUI_TLS_SERVER_NAME: &str = "sui";

pub struct Server {
    server: sui_http::ServerHandle,
    local_addr: Multiaddr,
    health_reporter: tonic_health::server::HealthReporter,
}

impl Server {
    pub async fn serve(self) -> Result<(), tonic::transport::Error> {
        self.server.wait_for_shutdown().await;
        Ok(())
    }

    pub fn local_addr(&self) -> &Multiaddr {
        &self.local_addr
    }

    pub fn health_reporter(&self) -> tonic_health::server::HealthReporter {
        self.health_reporter.clone()
    }

    pub fn handle(&self) -> &sui_http::ServerHandle {
        &self.server
    }
}

fn update_tcp_port_in_multiaddr(addr: &Multiaddr, port: u16) -> Multiaddr {
    addr.replace(1, |protocol| {
        if let Protocol::Tcp(_) = protocol {
            Some(Protocol::Tcp(port))
        } else {
            panic!("expected tcp protocol at index 1");
        }
    })
    .expect("tcp protocol at index 1")
}

#[derive(Clone)]
struct RequestLifetimeLayer<M: MetricsCallbackProvider> {
    metrics_provider: M,
}

impl<M: MetricsCallbackProvider, S> Layer<S> for RequestLifetimeLayer<M> {
    type Service = RequestLifetime<M, S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestLifetime {
            inner,
            metrics_provider: self.metrics_provider.clone(),
            path: None,
        }
    }
}

#[derive(Clone)]
struct RequestLifetime<M: MetricsCallbackProvider, S> {
    inner: S,
    metrics_provider: M,
    path: Option<String>,
}

impl<M: MetricsCallbackProvider, S, RequestBody> Service<Request<RequestBody>>
    for RequestLifetime<M, S>
where
    S: Service<Request<RequestBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<RequestBody>) -> Self::Future {
        if self.path.is_none() {
            let path = request.uri().path().to_string();
            self.metrics_provider.on_start(&path);
            self.path = Some(path);
        }
        self.inner.call(request)
    }
}

impl<M: MetricsCallbackProvider, S> Drop for RequestLifetime<M, S> {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            self.metrics_provider.on_drop(path)
        }
    }
}

#[cfg(test)]
mod test {
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::traits::KeyPair;
    use mysten_network::Multiaddr;
    use mysten_network::config::Config;
    use mysten_network::metrics::MetricsCallbackProvider;
    use std::ops::Deref;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tonic::Code;
    use tonic_health::pb::HealthCheckRequest;
    use tonic_health::pb::health_client::HealthClient;

    #[tokio::test]
    async fn test_metrics_layer_successful() {
        #[derive(Clone)]
        struct Metrics {
            /// a flag to figure out whether the
            /// on_request method has been called.
            metrics_called: Arc<Mutex<bool>>,
        }

        impl MetricsCallbackProvider for Metrics {
            fn on_request(&self, path: String) {
                assert_eq!(path, "/grpc.health.v1.Health/Check");
            }

            fn on_response(
                &self,
                path: String,
                _latency: Duration,
                status: u16,
                grpc_status_code: Code,
            ) {
                assert_eq!(path, "/grpc.health.v1.Health/Check");
                assert_eq!(status, 200);
                assert_eq!(grpc_status_code, Code::Ok);
                let mut m = self.metrics_called.lock().unwrap();
                *m = true
            }
        }

        let metrics = Metrics {
            metrics_called: Arc::new(Mutex::new(false)),
        };

        let address: Multiaddr = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();
        let config = Config::new();
        let keypair = Ed25519KeyPair::generate(&mut rand::thread_rng());

        let server = super::ServerBuilder::from_config(&config, metrics.clone())
            .bind(
                &address,
                Some(sui_tls::create_rustls_server_config(
                    keypair.copy().private(),
                    "test".to_string(),
                )),
            )
            .await
            .unwrap();

        let address = server.local_addr().to_owned();
        let channel = config
            .connect(
                &address,
                sui_tls::create_rustls_client_config(
                    keypair.public().to_owned(),
                    "test".to_string(),
                    None,
                ),
            )
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);

        client
            .check(HealthCheckRequest {
                service: "".to_owned(),
            })
            .await
            .unwrap();

        server.server.shutdown().await;

        assert!(metrics.metrics_called.lock().unwrap().deref());
    }

    #[tokio::test]
    async fn test_metrics_layer_error() {
        #[derive(Clone)]
        struct Metrics {
            /// a flag to figure out whether the
            /// on_request method has been called.
            metrics_called: Arc<Mutex<bool>>,
        }

        impl MetricsCallbackProvider for Metrics {
            fn on_request(&self, path: String) {
                assert_eq!(path, "/grpc.health.v1.Health/Check");
            }

            fn on_response(
                &self,
                path: String,
                _latency: Duration,
                status: u16,
                grpc_status_code: Code,
            ) {
                assert_eq!(path, "/grpc.health.v1.Health/Check");
                assert_eq!(status, 200);
                // According to https://github.com/grpc/grpc/blob/master/doc/statuscodes.md#status-codes-and-their-use-in-grpc
                // code 5 is not_found , which is what we expect to get in this case
                assert_eq!(grpc_status_code, Code::NotFound);
                let mut m = self.metrics_called.lock().unwrap();
                *m = true
            }
        }

        let metrics = Metrics {
            metrics_called: Arc::new(Mutex::new(false)),
        };

        let address: Multiaddr = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();
        let config = Config::new();
        let keypair = Ed25519KeyPair::generate(&mut rand::thread_rng());

        let server = super::ServerBuilder::from_config(&config, metrics.clone())
            .bind(
                &address,
                Some(sui_tls::create_rustls_server_config(
                    keypair.copy().private(),
                    "test".to_string(),
                )),
            )
            .await
            .unwrap();
        let address = server.local_addr().to_owned();
        let channel = config
            .connect(
                &address,
                sui_tls::create_rustls_client_config(
                    keypair.public().to_owned(),
                    "test".to_string(),
                    None,
                ),
            )
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);

        // Call the healthcheck for a service that doesn't exist
        // that should give us back an error with code 5 (not_found)
        // https://github.com/grpc/grpc/blob/master/doc/statuscodes.md#status-codes-and-their-use-in-grpc
        let _ = client
            .check(HealthCheckRequest {
                service: "non-existing-service".to_owned(),
            })
            .await;

        server.server.shutdown().await;

        assert!(metrics.metrics_called.lock().unwrap().deref());
    }

    async fn test_multiaddr(address: Multiaddr) {
        let config = Config::new();
        let keypair = Ed25519KeyPair::generate(&mut rand::thread_rng());

        let server_handle = super::ServerBuilder::from_config(
            &config,
            mysten_network::metrics::DefaultMetricsCallbackProvider::default(),
        )
        .bind(
            &address,
            Some(sui_tls::create_rustls_server_config(
                keypair.copy().private(),
                "test".to_string(),
            )),
        )
        .await
        .unwrap();
        let address = server_handle.local_addr().to_owned();
        let channel = config
            .connect(
                &address,
                sui_tls::create_rustls_client_config(
                    keypair.public().to_owned(),
                    "test".to_string(),
                    None,
                ),
            )
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);

        client
            .check(HealthCheckRequest {
                service: "".to_owned(),
            })
            .await
            .unwrap();

        server_handle.server.shutdown().await;
    }

    #[tokio::test]
    async fn dns() {
        let address: Multiaddr = "/dns/localhost/tcp/0/http".parse().unwrap();
        test_multiaddr(address).await;
    }

    #[tokio::test]
    async fn ip4() {
        let address: Multiaddr = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();
        test_multiaddr(address).await;
    }

    #[tokio::test]
    async fn ip6() {
        let address: Multiaddr = "/ip6/::1/tcp/0/http".parse().unwrap();
        test_multiaddr(address).await;
    }
}
