// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::config::Config;
use std::time::Duration;

pub mod api;
pub mod discovery;
pub mod endpoint_manager;
pub mod randomness;
pub mod state_sync;
pub mod utils;
pub mod validator;

pub use tonic;

pub const DEFAULT_CONNECT_TIMEOUT_SEC: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(30);
pub const DEFAULT_HTTP2_KEEPALIVE_SEC: Duration = Duration::from_secs(5);

// Use shorter request timeout in test configurations to allow more retries during validator restarts
pub const TEST_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(10);

static REQUEST_TIMEOUT: std::sync::LazyLock<Duration> = std::sync::LazyLock::new(|| {
    if mysten_common::in_test_configuration() {
        TEST_REQUEST_TIMEOUT_SEC
    } else {
        DEFAULT_REQUEST_TIMEOUT_SEC
    }
});

pub fn default_mysten_network_config() -> Config {
    let mut net_config = mysten_network::config::Config::new();
    net_config.connect_timeout = Some(DEFAULT_CONNECT_TIMEOUT_SEC);
    net_config.request_timeout = Some(*REQUEST_TIMEOUT);
    net_config.http2_keepalive_interval = Some(DEFAULT_HTTP2_KEEPALIVE_SEC);
    net_config
}
