// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result};
use core::time::Duration;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};
use std::net::SocketAddr;
use tracing::debug;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyConfig {
    pub network: String,
    pub inventory_hostname: String,
    pub listen_address: SocketAddr,
    pub remote_write: RemoteWriteConfig,
    pub json_rpc: PeerValidationConfig,
    pub metrics_address: SocketAddr,
    pub histogram_address: SocketAddr,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteWriteConfig {
    // TODO upgrade to https
    /// the remote_write url to post data to
    #[serde(default = "remote_write_url")]
    pub url: String,
    /// username is used for posting data to the remote_write api
    pub username: String,
    pub password: String,

    /// Sets the maximum idle connection per host allowed in the pool.
    /// <https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.pool_max_idle_per_host>
    #[serde(default = "pool_max_idle_per_host_default")]
    pub pool_max_idle_per_host: usize,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PeerValidationConfig {
    /// url is the json-rpc url we use to obtain valid peers on the blockchain
    pub url: String,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub interval: Duration,
    /// if certificate_file and private_key are not provided, we'll create a self-signed
    /// cert using this hostname
    #[serde(default = "hostname_default")]
    pub hostname: Option<String>,

    /// incoming client connections to this proxy will be presented with this pub key
    /// please use an absolute path
    pub certificate_file: Option<String>,
    /// private key for tls
    /// please use an absolute path
    pub private_key: Option<String>,
}

/// the default idle worker per host (reqwest to remote write url call)
fn pool_max_idle_per_host_default() -> usize {
    8
}

/// the default hostname we will use if not provided
fn hostname_default() -> Option<String> {
    Some("localhost".to_string())
}

/// the default remote write url
fn remote_write_url() -> String {
    "http://metrics-gw.testnet.sui.io/api/v1/push".to_string()
}

/// load our config file from a path
pub fn load<P: AsRef<std::path::Path>, T: DeserializeOwned + Serialize>(path: P) -> Result<T> {
    let path = path.as_ref();
    debug!("Reading config from {:?}", path);
    Ok(serde_yaml::from_reader(
        std::fs::File::open(path).context(format!("cannot open {:?}", path))?,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_load() {
        const TEMPLATE: &str = include_str!("./data/config.yaml");

        let _template: ProxyConfig = serde_yaml::from_str(TEMPLATE).unwrap();
    }
}
