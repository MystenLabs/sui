// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// Enable indexing of transactions and objects
    ///
    /// This enables indexing of transactions and objects which allows for a slightly richer rpc
    /// api. There are some APIs which will be disabled/enabled based on this config while others
    /// (eg GetTransaction) will still be enabled regardless of this config but may return slight
    /// less data (eg GetTransaction won't return the checkpoint that includes the requested
    /// transaction).
    ///
    /// Defaults to `false`, with indexing and APIs which require indexes being disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_indexing: Option<bool>,

    /// Configure the address to listen on for https
    ///
    /// Defaults to `0.0.0.0:9443` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_address: Option<SocketAddr>,

    /// TLS configuration to use for https.
    ///
    /// If not provided then the node will not create an https service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
}

impl Config {
    pub fn enable_indexing(&self) -> bool {
        self.enable_indexing.unwrap_or(false)
    }

    pub fn https_address(&self) -> SocketAddr {
        self.https_address
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 9443)))
    }

    pub fn tls_config(&self) -> Option<&TlsConfig> {
        self.tls.as_ref()
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TlsConfig {
    /// File path to a PEM formatted TLS certificate chain
    cert: String,
    /// File path to a PEM formatted TLS private key
    key: String,
}

impl TlsConfig {
    pub fn cert(&self) -> &str {
        &self.cert
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}
