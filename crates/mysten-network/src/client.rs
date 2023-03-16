// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::Config,
    multiaddr::{parse_dns, parse_ip4, parse_ip6, Multiaddr, Protocol},
};
use eyre::{eyre, Context, Result};
use tonic::transport::{Channel, Endpoint, Uri};

pub async fn connect(address: &Multiaddr) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address)?.connect().await?;
    Ok(channel)
}

pub fn connect_lazy(address: &Multiaddr) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address)?.connect_lazy();
    Ok(channel)
}

pub(crate) async fn connect_with_config(address: &Multiaddr, config: &Config) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address)?
        .apply_config(config)
        .connect()
        .await?;
    Ok(channel)
}

pub(crate) fn connect_lazy_with_config(address: &Multiaddr, config: &Config) -> Result<Channel> {
    let channel = endpoint_from_multiaddr(address)?
        .apply_config(config)
        .connect_lazy();
    Ok(channel)
}

fn endpoint_from_multiaddr(addr: &Multiaddr) -> Result<MyEndpoint> {
    let mut iter = addr.iter();

    let channel = match iter.next().ok_or_else(|| eyre!("address is empty"))? {
        Protocol::Dns(_) => {
            let (dns_name, tcp_port, http_or_https) = parse_dns(addr)?;
            let uri = format!("{http_or_https}://{dns_name}:{tcp_port}");
            MyEndpoint::try_from_uri(uri)?
        }
        Protocol::Ip4(_) => {
            let (socket_addr, http_or_https) = parse_ip4(addr)?;
            let uri = format!("{http_or_https}://{socket_addr}");
            MyEndpoint::try_from_uri(uri)?
        }
        Protocol::Ip6(_) => {
            let (socket_addr, http_or_https) = parse_ip6(addr)?;
            let uri = format!("{http_or_https}://{socket_addr}");
            MyEndpoint::try_from_uri(uri)?
        }
        // Protocol::Memory(_) => todo!(),
        #[cfg(unix)]
        Protocol::Unix(_) => {
            let (path, http_or_https) = crate::multiaddr::parse_unix(addr)?;
            let uri = format!("{http_or_https}://localhost");
            MyEndpoint::try_from_uri(uri)?.with_uds_connector(path.as_ref().into())
        }
        unsupported => return Err(eyre!("unsupported protocol {unsupported}")),
    };

    Ok(channel)
}

struct MyEndpoint {
    endpoint: Endpoint,
    #[cfg(unix)]
    uds_connector: Option<std::path::PathBuf>,
}

impl MyEndpoint {
    fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            #[cfg(unix)]
            uds_connector: None,
        }
    }

    fn try_from_uri(uri: String) -> Result<Self> {
        let uri: Uri = uri
            .parse()
            .with_context(|| format!("unable to create Uri from '{uri}'"))?;
        let endpoint = Endpoint::from(uri);
        Ok(Self::new(endpoint))
    }

    #[cfg(unix)]
    fn with_uds_connector(self, path: std::path::PathBuf) -> Self {
        Self {
            endpoint: self.endpoint,
            uds_connector: Some(path),
        }
    }

    fn apply_config(mut self, config: &Config) -> Self {
        self.endpoint = apply_config_to_endpoint(config, self.endpoint);
        self
    }

    fn connect_lazy(self) -> Channel {
        #[cfg(unix)]
        if let Some(path) = self.uds_connector {
            return self
                .endpoint
                .connect_with_connector_lazy(tower::service_fn(move |_: Uri| {
                    let path = path.clone();

                    // Connect to a Uds socket
                    tokio::net::UnixStream::connect(path)
                }));
        }

        self.endpoint.connect_lazy()
    }

    async fn connect(self) -> Result<Channel> {
        #[cfg(unix)]
        if let Some(path) = self.uds_connector {
            return self
                .endpoint
                .connect_with_connector(tower::service_fn(move |_: Uri| {
                    let path = path.clone();

                    // Connect to a Uds socket
                    tokio::net::UnixStream::connect(path)
                }))
                .await
                .map_err(Into::into);
        }

        self.endpoint.connect().await.map_err(Into::into)
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
