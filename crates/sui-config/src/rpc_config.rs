// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcConfig {
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
    pub tls: Option<RpcTlsConfig>,

    /// Maxumum budget for rendering a Move value into JSON.
    ///
    /// This sets the numbers of bytes that we are willing to spend on rendering field names and
    /// values when rendering a Move value into a JSON value.
    ///
    /// Defaults to `1MiB` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_json_move_value_size: Option<usize>,

    /// Configuration for RPC index initialization and bulk loading
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_initialization: Option<RpcIndexInitConfig>,

    /// Enable indexing of authenticated events
    ///
    /// This controls whether authenticated events are indexed and whether the authenticated
    /// events API endpoints are available. When disabled, authenticated events are not indexed
    /// and API calls will return an unsupported error.
    ///
    /// Defaults to `false`, with authenticated events indexing and API disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated_events_indexing: Option<bool>,
}

impl RpcConfig {
    pub fn enable_indexing(&self) -> bool {
        self.enable_indexing.unwrap_or(false)
    }

    pub fn https_address(&self) -> SocketAddr {
        self.https_address
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 9443)))
    }

    pub fn tls_config(&self) -> Option<&RpcTlsConfig> {
        self.tls.as_ref()
    }

    pub fn max_json_move_value_size(&self) -> usize {
        self.max_json_move_value_size.unwrap_or(1024 * 1024)
    }

    pub fn index_initialization_config(&self) -> Option<&RpcIndexInitConfig> {
        self.index_initialization.as_ref()
    }

    pub fn authenticated_events_indexing(&self) -> bool {
        self.authenticated_events_indexing.unwrap_or(false)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcTlsConfig {
    /// File path to a PEM formatted TLS certificate chain
    cert: String,
    /// File path to a PEM formatted TLS private key
    key: String,
}

impl RpcTlsConfig {
    pub fn cert(&self) -> &str {
        &self.cert
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

/// Configuration for RPC index initialization and bulk loading
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcIndexInitConfig {
    /// Override for RocksDB's set_db_write_buffer_size during bulk indexing.
    /// This is the total memory budget for all column families' memtables.
    ///
    /// Defaults to 90% of system RAM if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_write_buffer_size: Option<usize>,

    /// Override for each column family's write buffer size during bulk indexing.
    ///
    /// Defaults to 25% of system RAM divided by max_write_buffer_number if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_write_buffer_size: Option<usize>,

    /// Override for the maximum number of write buffers per column family during bulk indexing.
    /// This value is capped at 32 as an upper bound.
    ///
    /// Defaults to a dynamic value based on system RAM if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_max_write_buffer_number: Option<i32>,

    /// Override for the number of background jobs during bulk indexing.
    ///
    /// Defaults to the number of CPU cores if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_background_jobs: Option<i32>,

    /// Override for the batch size limit during bulk indexing.
    /// This controls how much data is accumulated in memory before flushing to disk.
    ///
    /// Defaults to half the write buffer size or 128MB, whichever is smaller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size_limit: Option<usize>,
}
