// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

const DEFAULT_HTTP2_KEEPALIVE_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone)]
pub struct Config {
    init_stream_window_size: Option<u32>,
    init_connection_window_size: Option<u32>,
    max_concurrent_streams: Option<u32>,
    pub(crate) tcp_keepalive: Option<Duration>,
    pub(crate) tcp_nodelay: bool,
    http2_keepalive_interval: Option<Duration>,
    http2_keepalive_timeout: Option<Duration>,
    http2_adaptive_window: Option<bool>,
    http2_max_pending_accept_reset_streams: Option<usize>,
    http2_max_header_list_size: Option<u32>,
    max_frame_size: Option<u32>,
    pub(crate) accept_http1: bool,
    enable_connect_protocol: bool,
    pub(crate) max_connection_age: Option<Duration>,
    pub(crate) allow_insecure: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            init_stream_window_size: None,
            init_connection_window_size: None,
            max_concurrent_streams: None,
            tcp_keepalive: None,
            tcp_nodelay: true,
            http2_keepalive_interval: None,
            http2_keepalive_timeout: None,
            http2_adaptive_window: None,
            http2_max_pending_accept_reset_streams: None,
            http2_max_header_list_size: None,
            max_frame_size: None,
            accept_http1: true,
            enable_connect_protocol: true,
            max_connection_age: None,
            allow_insecure: false,
        }
    }
}

impl Config {
    /// Sets the [`SETTINGS_INITIAL_WINDOW_SIZE`][spec] option for HTTP2
    /// stream-level flow control.
    ///
    /// Default is 65,535
    ///
    /// [spec]: https://httpwg.org/specs/rfc9113.html#InitialWindowSize
    pub fn initial_stream_window_size(self, sz: impl Into<Option<u32>>) -> Self {
        Self {
            init_stream_window_size: sz.into(),
            ..self
        }
    }

    /// Sets the max connection-level flow control for HTTP2
    ///
    /// Default is 65,535
    pub fn initial_connection_window_size(self, sz: impl Into<Option<u32>>) -> Self {
        Self {
            init_connection_window_size: sz.into(),
            ..self
        }
    }

    /// Sets the [`SETTINGS_MAX_CONCURRENT_STREAMS`][spec] option for HTTP2
    /// connections.
    ///
    /// Default is no limit (`None`).
    ///
    /// [spec]: https://httpwg.org/specs/rfc9113.html#n-stream-concurrency
    pub fn max_concurrent_streams(self, max: impl Into<Option<u32>>) -> Self {
        Self {
            max_concurrent_streams: max.into(),
            ..self
        }
    }

    /// Sets the maximum time option in milliseconds that a connection may exist
    ///
    /// Default is no limit (`None`).
    pub fn max_connection_age(self, max_connection_age: Duration) -> Self {
        Self {
            max_connection_age: Some(max_connection_age),
            ..self
        }
    }

    /// Set whether HTTP2 Ping frames are enabled on accepted connections.
    ///
    /// If `None` is specified, HTTP2 keepalive is disabled, otherwise the duration
    /// specified will be the time interval between HTTP2 Ping frames.
    /// The timeout for receiving an acknowledgement of the keepalive ping
    /// can be set with [`Config::http2_keepalive_timeout`].
    ///
    /// Default is no HTTP2 keepalive (`None`)
    pub fn http2_keepalive_interval(self, http2_keepalive_interval: Option<Duration>) -> Self {
        Self {
            http2_keepalive_interval,
            ..self
        }
    }

    /// Sets a timeout for receiving an acknowledgement of the keepalive ping.
    ///
    /// If the ping is not acknowledged within the timeout, the connection will be closed.
    /// Does nothing if http2_keep_alive_interval is disabled.
    ///
    /// Default is 20 seconds.
    pub fn http2_keepalive_timeout(self, http2_keepalive_timeout: Option<Duration>) -> Self {
        Self {
            http2_keepalive_timeout,
            ..self
        }
    }

    /// Sets whether to use an adaptive flow control. Defaults to false.
    /// Enabling this will override the limits set in http2_initial_stream_window_size and
    /// http2_initial_connection_window_size.
    pub fn http2_adaptive_window(self, enabled: Option<bool>) -> Self {
        Self {
            http2_adaptive_window: enabled,
            ..self
        }
    }

    /// Configures the maximum number of pending reset streams allowed before a GOAWAY will be sent.
    ///
    /// This will default to whatever the default in h2 is. As of v0.3.17, it is 20.
    ///
    /// See <https://github.com/hyperium/hyper/issues/2877> for more information.
    pub fn http2_max_pending_accept_reset_streams(self, max: Option<usize>) -> Self {
        Self {
            http2_max_pending_accept_reset_streams: max,
            ..self
        }
    }

    /// Set whether TCP keepalive messages are enabled on accepted connections.
    ///
    /// If `None` is specified, keepalive is disabled, otherwise the duration
    /// specified will be the time to remain idle before sending TCP keepalive
    /// probes.
    ///
    /// Default is no keepalive (`None`)
    pub fn tcp_keepalive(self, tcp_keepalive: Option<Duration>) -> Self {
        Self {
            tcp_keepalive,
            ..self
        }
    }

    /// Set the value of `TCP_NODELAY` option for accepted connections. Enabled by default.
    pub fn tcp_nodelay(self, enabled: bool) -> Self {
        Self {
            tcp_nodelay: enabled,
            ..self
        }
    }

    /// Sets the max size of received header frames.
    ///
    /// This will default to whatever the default in hyper is. As of v1.4.1, it is 16 KiB.
    pub fn http2_max_header_list_size(self, max: impl Into<Option<u32>>) -> Self {
        Self {
            http2_max_header_list_size: max.into(),
            ..self
        }
    }

    /// Sets the maximum frame size to use for HTTP2.
    ///
    /// Passing `None` will do nothing.
    ///
    /// If not set, will default from underlying transport.
    pub fn max_frame_size(self, frame_size: impl Into<Option<u32>>) -> Self {
        Self {
            max_frame_size: frame_size.into(),
            ..self
        }
    }

    /// Allow this accepting http1 requests.
    ///
    /// Default is `true`.
    pub fn accept_http1(self, accept_http1: bool) -> Self {
        Config {
            accept_http1,
            ..self
        }
    }

    /// Allow accepting insecure connections when a tls_config is provided.
    ///
    /// This will allow clients to connect both using TLS as well as without TLS on the same
    /// network interface.
    ///
    /// Default is `false`.
    ///
    /// NOTE: This presently will only work for `tokio::net::TcpStream` IO connections
    pub fn allow_insecure(self, allow_insecure: bool) -> Self {
        Config {
            allow_insecure,
            ..self
        }
    }

    pub(crate) fn connection_builder(
        &self,
    ) -> hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor> {
        let mut builder =
            hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());

        if !self.accept_http1 {
            builder = builder.http2_only();
        }

        if self.enable_connect_protocol {
            builder.http2().enable_connect_protocol();
        }

        let http2_keepalive_timeout = self
            .http2_keepalive_timeout
            .unwrap_or_else(|| Duration::new(DEFAULT_HTTP2_KEEPALIVE_TIMEOUT_SECS, 0));

        builder
            .http2()
            .timer(hyper_util::rt::TokioTimer::new())
            .initial_connection_window_size(self.init_connection_window_size)
            .initial_stream_window_size(self.init_stream_window_size)
            .max_concurrent_streams(self.max_concurrent_streams)
            .keep_alive_interval(self.http2_keepalive_interval)
            .keep_alive_timeout(http2_keepalive_timeout)
            .adaptive_window(self.http2_adaptive_window.unwrap_or_default())
            .max_pending_accept_reset_streams(self.http2_max_pending_accept_reset_streams)
            .max_frame_size(self.max_frame_size);

        if let Some(max_header_list_size) = self.http2_max_header_list_size {
            builder.http2().max_header_list_size(max_header_list_size);
        }

        builder
    }
}
