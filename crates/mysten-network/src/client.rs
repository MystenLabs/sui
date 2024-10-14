// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::Config,
    multiaddr::{parse_dns, parse_ip4, parse_ip6, Multiaddr, Protocol},
};
use eyre::{eyre, Context, Result};
use hyper_util::client::legacy::connect::{dns::Name, HttpConnector};
use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    io,
    net::{SocketAddr, ToSocketAddrs},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{self, Poll},
    time::Instant,
    vec,
};
use tokio::task::JoinHandle;
use tokio_rustls::rustls::ClientConfig;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::Service;
use tracing::{info, trace};

pub async fn connect(address: &Multiaddr, tls_config: Option<ClientConfig>) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address, tls_config)?
        .connect()
        .await?;
    Ok(channel)
}

pub fn connect_lazy(address: &Multiaddr, tls_config: Option<ClientConfig>) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address, tls_config)?.connect_lazy();
    Ok(channel)
}

pub(crate) async fn connect_with_config(
    address: &Multiaddr,
    tls_config: Option<ClientConfig>,
    config: &Config,
) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address, tls_config)?
        .apply_config(config)
        .connect()
        .await?;
    Ok(channel)
}

pub(crate) fn connect_lazy_with_config(
    address: &Multiaddr,
    tls_config: Option<ClientConfig>,
    config: &Config,
) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address, tls_config)?
        .apply_config(config)
        .connect_lazy();
    Ok(channel)
}

fn endpoint_from_multiaddr(
    addr: &Multiaddr,
    tls_config: Option<ClientConfig>,
) -> Result<MyEndpoint> {
    let mut iter = addr.iter();

    let channel = match iter.next().ok_or_else(|| eyre!("address is empty"))? {
        Protocol::Dns(_) => {
            let (dns_name, tcp_port, http_or_https) = parse_dns(addr)?;
            let uri = format!("{http_or_https}://{dns_name}:{tcp_port}");
            MyEndpoint::try_from_uri(uri, tls_config)?
        }
        Protocol::Ip4(_) => {
            let (socket_addr, http_or_https) = parse_ip4(addr)?;
            let uri = format!("{http_or_https}://{socket_addr}");
            MyEndpoint::try_from_uri(uri, tls_config)?
        }
        Protocol::Ip6(_) => {
            let (socket_addr, http_or_https) = parse_ip6(addr)?;
            let uri = format!("{http_or_https}://{socket_addr}");
            MyEndpoint::try_from_uri(uri, tls_config)?
        }
        unsupported => return Err(eyre!("unsupported protocol {unsupported}")),
    };

    Ok(channel)
}

struct MyEndpoint {
    endpoint: Endpoint,
    tls_config: Option<ClientConfig>,
}

static DISABLE_CACHING_RESOLVER: OnceCell<bool> = OnceCell::new();

impl MyEndpoint {
    fn new(endpoint: Endpoint, tls_config: Option<ClientConfig>) -> Self {
        Self {
            endpoint,
            tls_config,
        }
    }

    fn try_from_uri(uri: String, tls_config: Option<ClientConfig>) -> Result<Self> {
        let uri: Uri = uri
            .parse()
            .with_context(|| format!("unable to create Uri from '{uri}'"))?;
        let endpoint = Endpoint::from(uri);
        Ok(Self::new(endpoint, tls_config))
    }

    fn apply_config(mut self, config: &Config) -> Self {
        self.endpoint = apply_config_to_endpoint(config, self.endpoint);
        self
    }

    fn connect_lazy(self) -> Channel {
        let disable_caching_resolver = *DISABLE_CACHING_RESOLVER.get_or_init(|| {
            let disable_caching_resolver = std::env::var("DISABLE_CACHING_RESOLVER").is_ok();
            info!("DISABLE_CACHING_RESOLVER: {disable_caching_resolver}");
            disable_caching_resolver
        });

        if disable_caching_resolver {
            if let Some(tls_config) = self.tls_config {
                self.endpoint.connect_with_connector_lazy(
                    hyper_rustls::HttpsConnectorBuilder::new()
                        .with_tls_config(tls_config)
                        .https_only()
                        .enable_http2()
                        .build(),
                )
            } else {
                self.endpoint.connect_lazy()
            }
        } else {
            let mut http = HttpConnector::new_with_resolver(CachingResolver::new());
            http.enforce_http(false);
            http.set_nodelay(true);
            http.set_keepalive(None);
            http.set_connect_timeout(None);

            if let Some(tls_config) = self.tls_config {
                let https = hyper_rustls::HttpsConnectorBuilder::new()
                    .with_tls_config(tls_config)
                    .https_only()
                    .enable_http1()
                    .wrap_connector(http);
                self.endpoint.connect_with_connector_lazy(https)
            } else {
                self.endpoint.connect_with_connector_lazy(http)
            }
        }
    }

    async fn connect(self) -> Result<Channel> {
        if let Some(tls_config) = self.tls_config {
            let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls_config)
                .https_only()
                .enable_http2()
                .build();
            self.endpoint
                .connect_with_connector(https_connector)
                .await
                .map_err(Into::into)
        } else {
            self.endpoint.connect().await.map_err(Into::into)
        }
    }
}

fn apply_config_to_endpoint(config: &Config, mut endpoint: Endpoint) -> Endpoint {
    if let Some(limit) = config.concurrency_limit_per_connection {
        endpoint = endpoint.concurrency_limit(limit);
    }

    if let Some(timeout) = config.request_timeout {
        endpoint = endpoint.timeout(timeout);
    }

    if let Some(timeout) = config.connect_timeout {
        endpoint = endpoint.connect_timeout(timeout);
    }

    if let Some(tcp_nodelay) = config.tcp_nodelay {
        endpoint = endpoint.tcp_nodelay(tcp_nodelay);
    }

    if let Some(http2_keepalive_interval) = config.http2_keepalive_interval {
        endpoint = endpoint.http2_keep_alive_interval(http2_keepalive_interval);
    }

    if let Some(http2_keepalive_timeout) = config.http2_keepalive_timeout {
        endpoint = endpoint.keep_alive_timeout(http2_keepalive_timeout);
    }

    if let Some((limit, duration)) = config.rate_limit {
        endpoint = endpoint.rate_limit(limit, duration);
    }

    endpoint
        .initial_stream_window_size(config.http2_initial_stream_window_size)
        .initial_connection_window_size(config.http2_initial_connection_window_size)
        .tcp_keepalive(config.tcp_keepalive)
}

type CacheEntry = (Instant, Vec<SocketAddr>);

/// A caching resolver based on hyper_util GaiResolver
#[derive(Clone)]
pub struct CachingResolver {
    cache: Arc<Mutex<HashMap<Name, CacheEntry>>>,
}

type SocketAddrs = vec::IntoIter<SocketAddr>;

pub struct CachingFuture {
    inner: JoinHandle<Result<SocketAddrs, io::Error>>,
}

impl CachingResolver {
    pub fn new() -> Self {
        CachingResolver {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for CachingResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Service<Name> for CachingResolver {
    type Response = SocketAddrs;
    type Error = io::Error;
    type Future = CachingFuture;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let blocking = {
            let cache = self.cache.clone();
            tokio::task::spawn_blocking(move || {
                let entry = cache.lock().unwrap().get(&name).cloned();

                if let Some((when, addrs)) = entry {
                    trace!("cached host={:?}", name.as_str());

                    if when.elapsed().as_secs() > 60 {
                        trace!("refreshing cache for host={:?}", name.as_str());
                        // Start a new task to update the cache later.
                        tokio::task::spawn_blocking(move || {
                            if let Ok(addrs) = (name.as_str(), 0).to_socket_addrs() {
                                let addrs: Vec<_> = addrs.collect();
                                trace!("updating cached host={:?}", name.as_str());
                                cache
                                    .lock()
                                    .unwrap()
                                    .insert(name, (Instant::now(), addrs.clone()));
                            }
                        });
                    }

                    Ok(addrs.into_iter())
                } else {
                    trace!("resolving host={:?}", name.as_str());
                    match (name.as_str(), 0).to_socket_addrs() {
                        Ok(addrs) => {
                            let addrs: Vec<_> = addrs.collect();
                            cache
                                .lock()
                                .unwrap()
                                .insert(name, (Instant::now(), addrs.clone()));
                            Ok(addrs.into_iter())
                        }
                        res => res,
                    }
                }
            })
        };

        CachingFuture { inner: blocking }
    }
}

impl fmt::Debug for CachingResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("CachingResolver")
    }
}

impl Future for CachingFuture {
    type Output = Result<SocketAddrs, io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx).map(|res| match res {
            Ok(Ok(addrs)) => Ok(addrs),
            Ok(Err(err)) => Err(err),
            Err(join_err) => {
                if join_err.is_cancelled() {
                    Err(io::Error::new(io::ErrorKind::Interrupted, join_err))
                } else {
                    panic!("background task failed: {:?}", join_err)
                }
            }
        })
    }
}

impl fmt::Debug for CachingFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("CachingFuture")
    }
}

impl Drop for CachingFuture {
    fn drop(&mut self) {
        self.inner.abort();
    }
}
