// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use connection_handler::OnConnectionClose;
use http::{Request, Response};
use hyper_util::service::TowerToHyperService;
use io::ServerIo;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tower::{Service, ServiceBuilder, ServiceExt};
use tracing::trace;

use self::body::BoxBody;
use self::connection_info::ActiveConnections;

pub use http;

pub mod body;
mod config;
mod connection_handler;
mod connection_info;
mod fuse;
mod io;
mod listener;

pub use config::Config;
pub use listener::Listener;
pub use listener::ListenerExt;

pub use connection_info::ConnectInfo;
pub use connection_info::ConnectionId;
pub use connection_info::ConnectionInfo;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;
/// h2 alpn in plain format for rustls.
const ALPN_H2: &[u8] = b"h2";
/// h1 alpn in plain format for rustls.
const ALPN_H1: &[u8] = b"http/1.1";

#[derive(Default)]
pub struct Builder {
    config: Config,
    tls_config: Option<tokio_rustls::rustls::ServerConfig>,
}

impl Builder {
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn tls_config(mut self, tls_config: tokio_rustls::rustls::ServerConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    pub fn serve<A, S, ResponseBody>(
        self,
        addr: A,
        service: S,
    ) -> Result<ServerHandle<std::net::SocketAddr>, BoxError>
    where
        A: std::net::ToSocketAddrs,
        S: Service<
                Request<BoxBody>,
                Response = Response<ResponseBody>,
                Error: Into<BoxError>,
                Future: Send,
            > + Clone
            + Send
            + 'static,
        ResponseBody: http_body::Body<Data = bytes::Bytes, Error: Into<BoxError>> + Send + 'static,
    {
        let listener = listener::TcpListenerWithOptions::new(
            addr,
            self.config.tcp_nodelay,
            self.config.tcp_keepalive,
        )?;

        Self::serve_with_listener(self, listener, service)
    }

    pub fn serve_with_listener<L, S, ResponseBody>(
        self,
        listener: L,
        service: S,
    ) -> Result<ServerHandle<L::Addr>, BoxError>
    where
        L: Listener,
        S: Service<
                Request<BoxBody>,
                Response = Response<ResponseBody>,
                Error: Into<BoxError>,
                Future: Send,
            > + Clone
            + Send
            + 'static,
        ResponseBody: http_body::Body<Data = bytes::Bytes, Error: Into<BoxError>> + Send + 'static,
    {
        let local_addr = listener.local_addr()?;
        let graceful_shutdown_token = tokio_util::sync::CancellationToken::new();
        let connections = ActiveConnections::default();

        let tls_config = self.tls_config.map(|mut tls| {
            tls.alpn_protocols.push(ALPN_H2.into());
            if self.config.accept_http1 {
                tls.alpn_protocols.push(ALPN_H1.into());
            }
            Arc::new(tls)
        });

        let server = Server {
            config: self.config,
            tls_config,
            listener,
            local_addr: local_addr.clone(),
            service: ServiceBuilder::new()
                .layer(tower::util::BoxCloneService::layer())
                .map_response(|response: Response<ResponseBody>| response.map(body::boxed))
                .map_err(Into::into)
                .service(service),
            pending_connections: JoinSet::new(),
            connection_handlers: JoinSet::new(),
            connections: connections.clone(),
            graceful_shutdown_token: graceful_shutdown_token.clone(),
        };

        let handle = ServerHandle(Arc::new(HandleInner {
            local_addr,
            connections,
            graceful_shutdown_token,
        }));

        tokio::spawn(server.serve());

        Ok(handle)
    }
}

#[derive(Debug)]
pub struct ServerHandle<A = std::net::SocketAddr>(Arc<HandleInner<A>>);

#[derive(Debug)]
struct HandleInner<A = std::net::SocketAddr> {
    /// The local address of the server.
    local_addr: A,
    connections: ActiveConnections<A>,
    graceful_shutdown_token: tokio_util::sync::CancellationToken,
}

impl<A> ServerHandle<A> {
    /// Returns the local address of the server
    pub fn local_addr(&self) -> &A {
        &self.0.local_addr
    }

    /// Trigger a graceful shutdown of the server
    pub fn graceful_shutdown(&self) {
        self.0.graceful_shutdown_token.cancel();
    }

    pub fn connections(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, HashMap<ConnectionId, ConnectionInfo<A>>> {
        self.0.connections.read().unwrap()
    }

    /// Returns the number of active connections the server is handling
    pub fn number_of_connections(&self) -> usize {
        self.connections().len()
    }
}

type ConnectingOutput<Io, Addr> = Result<(ServerIo<Io>, Addr), crate::BoxError>;

struct Server<L: Listener> {
    config: Config,
    tls_config: Option<Arc<tokio_rustls::rustls::ServerConfig>>,

    listener: L,
    local_addr: L::Addr,
    service: tower::util::BoxCloneService<Request<BoxBody>, Response<BoxBody>, crate::BoxError>,

    pending_connections: JoinSet<ConnectingOutput<L::Io, L::Addr>>,
    connection_handlers: JoinSet<()>,
    connections: ActiveConnections<L::Addr>,
    graceful_shutdown_token: tokio_util::sync::CancellationToken,
}

impl<L> Server<L>
where
    L: Listener,
{
    async fn serve(mut self) -> Result<(), BoxError> {
        loop {
            tokio::select! {
                _ = self.graceful_shutdown_token.cancelled() => {
                    trace!("signal received, shutting down");
                    break;
                },
                (io, remote_addr) = self.listener.accept() => {
                    self.handle_incomming(io, remote_addr);
                },
                Some(maybe_connection) = self.pending_connections.join_next() => {
                    // If a task panics, just propagate it
                    let (io, remote_addr) = match maybe_connection.unwrap() {
                        Ok((io, remote_addr)) => {
                            (io, remote_addr)
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "error accepting connection");
                            continue;
                        }
                    };

                    trace!("connection accepted");
                    self.handle_connection(io, remote_addr);
                },
                Some(connection_handler_output) = self.connection_handlers.join_next() => {
                    // If a task panics, just propagate it
                    let _: () = connection_handler_output.unwrap();
                },
            }
        }

        // Shutting down, wait for all connection handlers to finish
        self.shutdown().await;

        Ok(())
    }

    fn handle_incomming(&mut self, io: L::Io, remote_addr: L::Addr) {
        if let Some(tls) = self.tls_config.clone() {
            let tls_acceptor = TlsAcceptor::from(tls);
            let allow_insecure = self.config.allow_insecure;
            self.pending_connections.spawn(async move {
                if allow_insecure {
                    // XXX: If we want to allow for supporting insecure traffic from other types of
                    // io, we'll need to implement a generic peekable IO type
                    if let Some(tcp) =
                        <dyn std::any::Any>::downcast_ref::<tokio::net::TcpStream>(&io)
                    {
                        // Determine whether new connection is TLS.
                        let mut buf = [0; 1];
                        // `peek` blocks until at least some data is available, so if there is no error then
                        // it must return the one byte we are requesting.
                        tcp.peek(&mut buf).await?;
                        // First byte of a TLS handshake is 0x16, so if it isn't 0x16 then its
                        // insecure
                        if buf != [0x16] {
                            tracing::trace!("accepting insecure connection");
                            return Ok((ServerIo::new_io(io), remote_addr));
                        }
                    } else {
                        tracing::warn!("'allow_insecure' is configured but io type is not 'tokio::net::TcpStream'");
                    }
                }

                tracing::trace!("accepting TLS connection");
                let io = tls_acceptor.accept(io).await?;
                Ok((ServerIo::new_tls_io(io), remote_addr))
            });
        } else {
            self.handle_connection(ServerIo::new_io(io), remote_addr);
        }
    }

    fn handle_connection(&mut self, io: ServerIo<L::Io>, remote_addr: L::Addr) {
        let connection_shutdown_token = self.graceful_shutdown_token.child_token();
        let connection_info = ConnectionInfo::new(
            remote_addr,
            io.peer_certs(),
            connection_shutdown_token.clone(),
        );
        let connection_id = connection_info.id();
        let connect_info = connection_info::ConnectInfo {
            local_addr: self.local_addr.clone(),
            remote_addr: connection_info.remote_address().clone(),
        };
        let peer_certificates = connection_info.peer_certs_owned();
        let hyper_io = hyper_util::rt::TokioIo::new(io);

        let hyper_svc = TowerToHyperService::new(self.service.clone().map_request(
            move |mut request: Request<hyper::body::Incoming>| {
                request.extensions_mut().insert(connect_info.clone());
                if let Some(peer_certificates) = peer_certificates.clone() {
                    request.extensions_mut().insert(peer_certificates);
                }

                request.map(body::boxed)
            },
        ));

        self.connections
            .write()
            .unwrap()
            .insert(connection_id, connection_info);
        let on_connection_close = OnConnectionClose::new(connection_id, self.connections.clone());

        self.connection_handlers
            .spawn(connection_handler::serve_connection(
                hyper_io,
                hyper_svc,
                self.config.connection_builder(),
                connection_shutdown_token,
                self.config.max_connection_age,
                on_connection_close,
            ));
    }

    async fn shutdown(mut self) {
        // Just to be careful make sure the token is canceled
        self.graceful_shutdown_token.cancel();

        // Terminate any in-progress pending connections
        self.pending_connections.shutdown().await;

        // Wait for all connection handlers to terminate
        trace!(
            "waiting for {} connections to close",
            self.connection_handlers.len()
        );
        while self.connection_handlers.join_next().await.is_some() {}
    }
}

#[tokio::test]
async fn foo() {
    use axum::Router;

    // build our application with a single route
    let app = Router::new().route("/", axum::routing::get(|| async { "Hello, World!" }));

    // run our app with hyper, listening globally on port 3000
    let _handle = Builder {
        config: Default::default(),
        tls_config: None,
    }
    .serve("127.0.0.1:3000", app)
    .unwrap();

    dbg!(reqwest::get("http://127.0.0.1:3000").await.unwrap());
}
