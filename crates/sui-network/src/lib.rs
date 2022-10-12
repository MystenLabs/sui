// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::config::Config;
use std::time::Duration;

pub mod api;

pub use tonic;

pub fn default_mysten_network_config() -> Config {
    let mut net_config = mysten_network::config::Config::new();
    net_config.connect_timeout = Some(Duration::from_secs(5));
    net_config.request_timeout = Some(Duration::from_secs(5));
    net_config.http2_keepalive_interval = Some(Duration::from_secs(5));
    net_config
}
