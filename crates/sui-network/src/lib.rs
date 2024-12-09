// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::config::Config;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tracing::error;

pub mod api;
pub mod discovery;
pub mod randomness;
pub mod state_sync;
pub mod utils;

pub use tonic;

pub const DEFAULT_CONNECT_TIMEOUT_SEC: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(30);
pub const DEFAULT_HTTP2_KEEPALIVE_SEC: Duration = Duration::from_secs(5);

pub fn default_mysten_network_config() -> Config {
    let mut net_config = mysten_network::config::Config::new();
    net_config.connect_timeout = Some(DEFAULT_CONNECT_TIMEOUT_SEC);
    net_config.request_timeout = Some(DEFAULT_REQUEST_TIMEOUT_SEC);
    net_config.http2_keepalive_interval = Some(DEFAULT_HTTP2_KEEPALIVE_SEC);
    net_config
}

pub fn parse_ip(ip: &str) -> Option<IpAddr> {
    ip.parse::<IpAddr>().ok().or_else(|| {
        ip.parse::<SocketAddr>()
            .ok()
            .map(|socket_addr| socket_addr.ip())
            .or_else(|| {
                error!("Failed to parse value of {:?} to ip address or socket.", ip,);
                None
            })
    })
}
