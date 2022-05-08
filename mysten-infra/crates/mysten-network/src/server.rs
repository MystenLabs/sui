// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::Config,
    multiaddr::{parse_dns, parse_ip4, parse_ip6, parse_unix},
};
use anyhow::{anyhow, Result};
use multiaddr::{Multiaddr, Protocol};
use std::{convert::Infallible, net::SocketAddr};
use tokio::net::{TcpListener, ToSocketAddrs, UnixListener};
use tokio_stream::wrappers::{TcpListenerStream, UnixListenerStream};
use tonic::{
    body::BoxBody,
    codegen::{
        http::{Request, Response},
        BoxFuture,
    },
    transport::{server::Router, Body, NamedService},
};
use tower::{
    layer::util::{Identity, Stack},
    limit::GlobalConcurrencyLimitLayer,
    load_shed::LoadShedLayer,
    util::Either,
    Service, ServiceBuilder,
};

pub struct ServerBuilder {
    router: Router<WrapperService>,
    health_reporter: tonic_health::server::HealthReporter,
}

type WrapperService = Stack<
    Stack<
        Either<LoadShedLayer, Identity>,
        Stack<Either<GlobalConcurrencyLimitLayer, Identity>, Identity>,
    >,
    Identity,
>;

impl ServerBuilder {
    pub fn from_config(config: &Config) -> Self {
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

        let load_shed = if config.load_shed.unwrap_or_default() {
            Some(tower::load_shed::LoadShedLayer::new())
        } else {
            None
        };

        let global_concurrency_limit = config
            .global_concurrency_limit
            .map(tower::limit::GlobalConcurrencyLimitLayer::new);

        let layer = ServiceBuilder::new()
            .option_layer(global_concurrency_limit)
            .option_layer(load_shed)
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

        let (local_addr, server): (Multiaddr, BoxFuture<(), tonic::transport::Error>) =
            match iter.next().ok_or_else(|| anyhow!("malformed addr"))? {
                Protocol::Dns(_) => {
                    let (dns_name, tcp_port, _http_or_https) = parse_dns(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, (dns_name.as_ref(), tcp_port))
                            .await?;
                    let server = Box::pin(self.router.serve_with_incoming(incoming));
                    (local_addr, server)
                }
                Protocol::Ip4(_) => {
                    let (socket_addr, _http_or_https) = parse_ip4(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, socket_addr).await?;
                    let server = Box::pin(self.router.serve_with_incoming(incoming));
                    (local_addr, server)
                }
                Protocol::Ip6(_) => {
                    let (socket_addr, _http_or_https) = parse_ip6(addr)?;
                    let (local_addr, incoming) =
                        tcp_listener_and_update_multiaddr(addr, socket_addr).await?;
                    let server = Box::pin(self.router.serve_with_incoming(incoming));
                    (local_addr, server)
                }
                // Protocol::Memory(_) => todo!(),
                #[cfg(unix)]
                Protocol::Unix(_) => {
                    let (path, _http_or_https) = parse_unix(addr)?;
                    let uds = UnixListener::bind(path.as_ref())?;
                    let uds_stream = UnixListenerStream::new(uds);
                    let local_addr = addr.to_owned();
                    let server = Box::pin(self.router.serve_with_incoming(uds_stream));
                    (local_addr, server)
                }
                unsupported => return Err(anyhow!("unsupported protocol {unsupported}")),
            };

        Ok(Server {
            server,
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
