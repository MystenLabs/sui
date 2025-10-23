// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DurationSeconds;
use std::{net::SocketAddr, time::Duration};
use tracing::error;
use url::Url;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyConfig {
    pub listen_address: SocketAddr,
    pub metrics_address: SocketAddr,
    /// The address of the fullnode to proxy requests to.
    pub fullnode_address: Url,
    /// Methods that are not proxied to the fullnode.
    pub unsupported_methods: Vec<String>,
    /// The list of origins that are allowed to access the proxy.
    pub allowed_origins: Option<Vec<String>>,
    /// The size of the cache for pagination cursors.
    pub cursor_cache_size: u64,
    /// Maximum number of idle connections to keep in the connection pool.
    /// When set, this limits the number of connections that remain open but unused,
    /// helping to conserve system resources.
    #[serde(default = "default_max_idle_connections")]
    pub max_idle_connections: usize,
    /// Idle timeout for connections in the connection pool.
    /// This should be set to a value less than the keep-alive timeout of the server to avoid sending requests to a closed connection.
    /// if your you expect sui-edge-proxy to recieve a small number of requests per second, you should set this to a higher value.
    #[serde_as(as = "DurationSeconds")]
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: Duration,
}

fn default_max_idle_connections() -> usize {
    100
}

fn default_idle_timeout() -> Duration {
    Duration::from_secs(60)
}

/// Load and validate configuration
pub async fn load<P: AsRef<std::path::Path>>(path: P) -> Result<(ProxyConfig, Client)> {
    let path = path.as_ref();
    let config: ProxyConfig = serde_yaml::from_reader(
        std::fs::File::open(path).context(format!("cannot open {:?}", path))?,
    )?;

    // Build a reqwest client that supports HTTP/2
    let client = reqwest::ClientBuilder::new()
        .http2_prior_knowledge()
        .http2_keep_alive_while_idle(true)
        .pool_idle_timeout(config.idle_timeout_seconds)
        .pool_max_idle_per_host(config.max_idle_connections)
        .build()
        .expect("Failed to build HTTP/2 client");

    validate_fullnode_url(&client, &config.fullnode_address).await?;

    Ok((config, client))
}

/// Validate that the given PeerConfig URL has a valid host
async fn validate_fullnode_url(client: &Client, fullnode_address: &Url) -> Result<()> {
    let health_url = fullnode_address
        .join("/health")
        .context("Failed to construct health check URL")?;

    const RETRY_DELAY: Duration = Duration::from_secs(1);
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

    let mut attempt = 1;
    loop {
        match client
            .get(health_url.clone())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
        {
            Ok(response) => {
                if response.version() != reqwest::Version::HTTP_2 {
                    tracing::warn!(
                        "Fullnode {:?} does not support HTTP/2 (using {:?})",
                        fullnode_address,
                        response.version()
                    );
                }

                if !response.status().is_success() {
                    tracing::warn!(
                        "Health check failed for fullnode {:?} with status {}",
                        fullnode_address,
                        response.status()
                    );
                }
                return Ok(());
            }
            Err(e) => {
                error!(
                    "Failed to connect to fullnode {:?} (attempt {}): {}",
                    fullnode_address, attempt, e
                );
                tokio::time::sleep(RETRY_DELAY).await;
                attempt += 1;
                continue;
            }
        }
    }
}
