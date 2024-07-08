// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::{
    DefaultMetricsCallbackProvider, MetricsCallbackProvider, MetricsHandler,
    GRPC_ENDPOINT_PATH_HEADER,
};
use crate::{
    config::Config,
    multiaddr::{parse_dns, parse_ip4, parse_ip6, Multiaddr, Protocol},
};
use eyre::{eyre, Result};
use futures::FutureExt;
use std::task::{Context, Poll};
use std::{convert::Infallible, net::SocketAddr};
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codegen::http::HeaderValue;
use tonic::{
    body::BoxBody,
    codegen::{
        http::{Request, Response},
        BoxFuture,
    },
    server::NamedService,
    transport::{server::Router, Body},
};
use tower::{
    layer::util::{Identity, Stack},
    limit::GlobalConcurrencyLimitLayer,
    load_shed::LoadShedLayer,
    util::Either,
    Layer, Service, ServiceBuilder,
};
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::propagate_header::PropagateHeaderLayer;
use tower_http::set_header::SetRequestHeaderLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnBodyChunk, DefaultOnEos, TraceLayer};

pub struct ServerBuilder<M: MetricsCallbackProvider = DefaultMetricsCallbackProvider> {
    router: Router<WrapperService<M>>,
    health_reporter: tonic_health::server::HealthReporter,
}

type AddPathToHeaderFunction = fn(&Request<Body>) -> Option<HeaderValue>;

type WrapperService<M> = Stack<
    Stack<
        PropagateHeaderLayer,
        Stack<
            TraceLayer<
                SharedClassifier<GrpcErrorsAsFailures>,
                DefaultMakeSpan,
                MetricsHandler<M>,
                MetricsHandler<M>,
                DefaultOnBodyChunk,
                DefaultOnEos,
                MetricsHandler<M>,
            >,
            Stack<
                SetRequestHeaderLayer<AddPathToHeaderFunction>,
                Stack<
                    RequestLifetimeLayer<M>,
                    Stack<
                        Either<LoadShedLayer, Identity>,
                        Stack<Either<GlobalConcurrencyLimitLayer, Identity>, Identity>,
                    >,
                >,
            >,
        >,
    >,
    Identity,
>;

impl<M: MetricsCallbackProvider> ServerBuilder<M> {
    pub fn from_config(config: &Config, metrics_provider: M) -> Self {
        let mut builder = tonic::transport::server::Server::builder();

        if let Some(limit) = config.concurrency_limit_per_connection {
            builder = builder.concurrency_limit_per_connection(limit);
        }

        if let Some(timeout) = config.request_timeout {
            builder = builder.timeout(timeout);
        }

        if let Some(tcp_nodelay) = config.tcp_nodelay {
            builder = builder.tcp_nodelay(tcp_nodelay);
        }

        let load_shed = config
            .load_shed
            .unwrap_or_default()
            .then_some(tower::load_shed::LoadShedLayer::new());

        let metrics = MetricsHandler::new(metrics_provider.clone());

        let request_metrics = TraceLayer::new_for_grpc()
            .on_request(metrics.clone())
            .on_response(metrics.clone())
            .on_failure(metrics);

        let global_concurrency_limit = config
            .global_concurrency_limit
            .map(tower::limit::GlobalConcurrencyLimitLayer::new);

        fn add_path_to_request_header(request: &Request<Body>) -> Option<HeaderValue> {
            let path = request.uri().path();
            Some(HeaderValue::from_str(path).unwrap())
        }

        let layer = ServiceBuilder::new()
            .option_layer(global_concurrency_limit)
            .option_layer(load_shed)
            .layer(RequestLifetimeLayer { metrics_provider })
            .layer(SetRequestHeaderLayer::overriding(
                GRPC_ENDPOINT_PATH_HEADER.clone(),
                add_path_to_request_header as AddPathToHeaderFunction,
            ))
            .layer(request_metrics)
            .layer(PropagateHeaderLayer::new(GRPC_ENDPOINT_PATH_HEADER.clone()))
            .into_inner();

        let (health_reporter, health_service) = tonic_health::server::health_reporter();
        let router = builder
            .initial_stream_window_size(config.http2_initial_stream_window_size)
            .initial_connection_window_size(config.http2_initial_connection_window_size)
            .http2_keepalive_interval(config.http2_keepalive_interval)
            .http2_keepalive_timeout(config.http2_keepalive_timeout)
            .max_concurrent_streams(config.http2_max_concurrent_streams)
            .tcp_keepalive(config.tcp_keepalive)
            .layer(layer)
            .add_service(health_service);

        Self {
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
        S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible>
            + NamedService
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        self.router = self.router.add_service(svc);
        self
    }

    pub async fn bind(self, addr: &Multiaddr) -> Result<Server> {
        let mut iter = addr.iter();

        let (tx_cancellation, rx_cancellation) = tokio::sync::oneshot::channel();
        let rx_cancellation = rx_cancellation.map(|_| ());
        let (local_addr, server): (Multiaddr, BoxFuture<(), tonic::transport::Error>) =
            match iter.next().ok_or_else(|| eyre!("malformed addr"))? {
                Protocol::Dns(_) => {
                    let (dns_name, tcp_port, _http_or_https) = parse_dns(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, (dns_name.as_ref(), tcp_port))
                            .await?;
                    let server = Box::pin(
                        self.router
                            .serve_with_incoming_shutdown(incoming, rx_cancellation),
                    );
                    (local_addr, server)
                }
                Protocol::Ip4(_) => {
                    let (socket_addr, _http_or_https) = parse_ip4(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, socket_addr).await?;
                    let server = Box::pin(
                        self.router
                            .serve_with_incoming_shutdown(incoming, rx_cancellation),
                    );
                    (local_addr, server)
                }
                Protocol::Ip6(_) => {
                    let (socket_addr, _http_or_https) = parse_ip6(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, socket_addr).await?;
                    let server = Box::pin(
                        self.router
                            .serve_with_incoming_shutdown(incoming, rx_cancellation),
                    );
                    (local_addr, server)
                }
                // Protocol::Memory(_) => todo!(),
                #[cfg(unix)]
                Protocol::Unix(_) => {
                    let (path, _http_or_https) = crate::multiaddr::parse_unix(addr)?;
                    let uds = tokio::net::UnixListener::bind(path.as_ref())?;
                    let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);
                    let local_addr = addr.to_owned();
                    let server = Box::pin(
                        self.router
                            .serve_with_incoming_shutdown(uds_stream, rx_cancellation),
                    );
                    (local_addr, server)
                }
                unsupported => return Err(eyre!("unsupported protocol {unsupported}")),
            };

        Ok(Server {
            server,
            cancel_handle: Some(tx_cancellation),
            local_addr,
            health_reporter: self.health_reporter,
        })
    }
}

async fn tcp_listener_and_update_multiaddr<T: ToSocketAddrs>(
    address: &Multiaddr,
    socket_addr: T,
) -> Result<(Multiaddr, TcpListenerStream)> {
    let (local_addr, incoming) = tcp_listener(socket_addr).await?;
    let local_addr = update_tcp_port_in_multiaddr(address, local_addr.port());
    Ok((local_addr, incoming))
}

async fn tcp_listener<T: ToSocketAddrs>(address: T) -> Result<(SocketAddr, TcpListenerStream)> {
    let listener = TcpListener::bind(address).await?;
    let local_addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    Ok((local_addr, incoming))
}

pub struct Server {
    server: BoxFuture<(), tonic::transport::Error>,
    cancel_handle: Option<tokio::sync::oneshot::Sender<()>>,
    local_addr: Multiaddr,
    health_reporter: tonic_health::server::HealthReporter,
}

impl Server {
    pub async fn serve(self) -> Result<(), tonic::transport::Error> {
        self.server.await
    }

    pub fn local_addr(&self) -> &Multiaddr {
        &self.local_addr
    }

    pub fn health_reporter(&self) -> tonic_health::server::HealthReporter {
        self.health_reporter.clone()
    }

    pub fn take_cancel_handle(&mut self) -> Option<tokio::sync::oneshot::Sender<()>> {
        self.cancel_handle.take()
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

#[cfg(test)]
mod test {
    use crate::config::Config;
    use crate::metrics::MetricsCallbackProvider;
    use crate::Multiaddr;
    use std::ops::Deref;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tonic::Code;
    use tonic_health::pb::health_client::HealthClient;
    use tonic_health::pb::HealthCheckRequest;

    #[test]
    fn document_multiaddr_limitation_for_unix_protocol() {
        // You can construct a multiaddr by hand (ie binary format) just fine
        let path = "/tmp/foo";
        let addr = Multiaddr::new_internal(multiaddr::multiaddr!(Unix(path), Http));

        // But it doesn't round-trip in the human readable format
        let s = addr.to_string();
        assert!(s.parse::<Multiaddr>().is_err());
    }

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

        let mut server = config
            .server_builder_with_metrics(metrics.clone())
            .bind(&address)
            .await
            .unwrap();

        let address = server.local_addr().to_owned();
        let cancel_handle = server.take_cancel_handle().unwrap();
        let server_handle = tokio::spawn(server.serve());
        let channel = config.connect(&address).await.unwrap();
        let mut client = HealthClient::new(channel);

        client
            .check(HealthCheckRequest {
                service: "".to_owned(),
            })
            .await
            .unwrap();

        cancel_handle.send(()).unwrap();
        server_handle.await.unwrap().unwrap();

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

        let mut server = config
            .server_builder_with_metrics(metrics.clone())
            .bind(&address)
            .await
            .unwrap();

        let address = server.local_addr().to_owned();
        let cancel_handle = server.take_cancel_handle().unwrap();
        let server_handle = tokio::spawn(server.serve());
        let channel = config.connect(&address).await.unwrap();
        let mut client = HealthClient::new(channel);

        // Call the healthcheck for a service that doesn't exist
        // that should give us back an error with code 5 (not_found)
        // https://github.com/grpc/grpc/blob/master/doc/statuscodes.md#status-codes-and-their-use-in-grpc
        let _ = client
            .check(HealthCheckRequest {
                service: "non-existing-service".to_owned(),
            })
            .await;

        cancel_handle.send(()).unwrap();
        server_handle.await.unwrap().unwrap();

        assert!(metrics.metrics_called.lock().unwrap().deref());
    }

    async fn test_multiaddr(address: Multiaddr) {
        let config = Config::new();
        let mut server = config.server_builder().bind(&address).await.unwrap();
        let address = server.local_addr().to_owned();
        let cancel_handle = server.take_cancel_handle().unwrap();
        let server_handle = tokio::spawn(server.serve());
        let channel = config.connect(&address).await.unwrap();
        let mut client = HealthClient::new(channel);

        client
            .check(HealthCheckRequest {
                service: "".to_owned(),
            })
            .await
            .unwrap();

        cancel_handle.send(()).unwrap();
        server_handle.await.unwrap().unwrap();
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

    #[cfg(unix)]
    #[tokio::test]
    async fn unix() {
        // Note that this only works when constructing a multiaddr by hand and not via the
        // human-readable format
        let path = "unix-domain-socket";
        let address = Multiaddr::new_internal(multiaddr::multiaddr!(Unix(path), Http));
        test_multiaddr(address).await;
        std::fs::remove_file(path).unwrap();
    }
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
