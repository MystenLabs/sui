// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::{DefaultMetricsCallbackProvider, MetricsCallbackProvider};
use crate::{
    client::{connect_lazy_with_config, connect_with_config},
    server::ServerBuilder,
    Multiaddr,
};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_rustls::rustls::ClientConfig;
use tonic::transport::Channel;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    /// Set the concurrency limit applied to inbound requests per connection.
    pub concurrency_limit_per_connection: Option<usize>,

    /// Set a timeout for all request handlers.
    pub request_timeout: Option<Duration>,

    /// Set a timeout for establishing an outbound connection.
    pub connect_timeout: Option<Duration>,

    /// Sets the SETTINGS_INITIAL_WINDOW_SIZE option for HTTP2 stream-level flow control.
    /// Default is 65,535
    pub http2_initial_stream_window_size: Option<u32>,

    /// Sets the max connection-level flow control for HTTP2
    ///
    /// Default is 65,535
    pub http2_initial_connection_window_size: Option<u32>,

    /// Sets the SETTINGS_MAX_CONCURRENT_STREAMS option for HTTP2 connections.
    ///
    /// Default is no limit (None).
    pub http2_max_concurrent_streams: Option<u32>,

    /// Set whether TCP keepalive messages are enabled on accepted connections.
    ///
    /// If None is specified, keepalive is disabled, otherwise the duration specified will be the
    /// time to remain idle before sending TCP keepalive probes.
    ///
    /// Default is no keepalive (None)
    pub tcp_keepalive: Option<Duration>,

    /// Set the value of TCP_NODELAY option for accepted connections. Enabled by default.
    pub tcp_nodelay: Option<bool>,

    /// Set whether HTTP2 Ping frames are enabled on accepted connections.
    ///
    /// If None is specified, HTTP2 keepalive is disabled, otherwise the duration specified will be
    /// the time interval between HTTP2 Ping frames. The timeout for receiving an acknowledgement
    /// of the keepalive ping can be set with http2_keepalive_timeout.
    ///
    /// Default is no HTTP2 keepalive (None)
    pub http2_keepalive_interval: Option<Duration>,

    /// Sets a timeout for receiving an acknowledgement of the keepalive ping.
    ///
    /// If the ping is not acknowledged within the timeout, the connection will be closed. Does nothing
    /// if http2_keep_alive_interval is disabled.
    ///
    /// Default is 20 seconds.
    pub http2_keepalive_timeout: Option<Duration>,

    // Only affects servers
    pub load_shed: Option<bool>,

    /// Only affects clients
    pub rate_limit: Option<(u64, Duration)>,

    // Only affects servers
    pub global_concurrency_limit: Option<usize>,
}

impl Config {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn server_builder(&self) -> ServerBuilder {
        ServerBuilder::from_config(self, DefaultMetricsCallbackProvider::default())
    }

    pub fn server_builder_with_metrics<M>(&self, metrics_provider: M) -> ServerBuilder<M>
    where
        M: MetricsCallbackProvider,
    {
        ServerBuilder::from_config(self, metrics_provider)
    }

    pub async fn connect(
        &self,
        addr: &Multiaddr,
        tls_config: Option<ClientConfig>,
    ) -> Result<Channel> {
        connect_with_config(addr, tls_config, self).await
    }

    pub fn connect_lazy(
        &self,
        addr: &Multiaddr,
        tls_config: Option<ClientConfig>,
    ) -> Result<Channel> {
        connect_lazy_with_config(addr, tls_config, self)
    }

    pub(crate) fn http_config(&self) -> sui_http::Config {
        sui_http::Config::default()
            .initial_stream_window_size(self.http2_initial_stream_window_size)
            .initial_connection_window_size(self.http2_initial_connection_window_size)
            .max_concurrent_streams(self.http2_max_concurrent_streams)
            .http2_keepalive_timeout(self.http2_keepalive_timeout)
            .http2_keepalive_interval(self.http2_keepalive_interval)
            .tcp_keepalive(self.tcp_keepalive)
            .tcp_nodelay(self.tcp_nodelay.unwrap_or_default())
    }
}
