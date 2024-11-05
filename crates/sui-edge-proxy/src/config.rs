// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyConfig {
    pub listen_address: SocketAddr,
    pub metrics_address: SocketAddr,
    pub execution_peer: PeerConfig,
    pub read_peer: PeerConfig,
    pub max_idle_connections: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PeerConfig {
    pub address: Url,
}

/// Load and validate configuration
pub fn load<P: AsRef<std::path::Path>>(path: P) -> Result<ProxyConfig> {
    let path = path.as_ref();
    let config: ProxyConfig = serde_yaml::from_reader(
        std::fs::File::open(path).context(format!("cannot open {:?}", path))?,
    )?;

    validate_peer_url(&config.read_peer)?;
    validate_peer_url(&config.execution_peer)?;

    Ok(config)
}

/// Validate that the given PeerConfig URL has a valid host
fn validate_peer_url(peer: &PeerConfig) -> Result<()> {
    if peer.address.host_str().is_none() {
        Err(anyhow!(
            "URL '{}' does not contain a valid host",
            peer.address
        ))
    } else {
        Ok(())
    }
}
