// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Public error types for `sui-forking` programmatic APIs.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Configuration errors returned while building [`crate::ForkingNodeConfig`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Returned when a network string cannot be parsed as a known network or valid URL.
    #[error("invalid network '{value}': {reason}")]
    InvalidNetwork {
        /// User-provided raw value.
        value: String,
        /// Human-readable parse error.
        reason: String,
    },

    /// Returned when `--network` is custom and fullnode URL is missing.
    #[error("fullnode_url is required when network is custom")]
    MissingFullnodeUrlForCustomNetwork,

    /// Returned when a URL uses a non-http(s) scheme.
    #[error("unsupported URL scheme '{scheme}' for {field}; expected http or https")]
    InvalidUrlScheme {
        /// Field name being validated.
        field: &'static str,
        /// The invalid scheme.
        scheme: String,
    },

    /// Returned when a configured port is zero.
    #[error("{field} must be non-zero")]
    InvalidPort {
        /// Field name being validated.
        field: &'static str,
    },

    /// Returned when a configured data directory is empty.
    #[error("data_dir cannot be empty")]
    EmptyDataDir,
}

/// Runtime startup and lifecycle errors for [`crate::ForkingNode`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StartError {
    /// Error while validating configuration.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// Failed to create the configured data directory.
    #[error("failed to create data directory at {path}")]
    CreateDataDir {
        /// Directory path.
        path: PathBuf,
        /// Source OS error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to create a temporary directory when no data directory was configured.
    #[error("failed to create temporary data directory: {message}")]
    CreateTempDir {
        /// Detailed error message.
        message: String,
    },

    /// Startup task exited before it reported readiness.
    #[error("forking server exited before readiness: {message}")]
    ExitedBeforeReady {
        /// Detailed error message.
        message: String,
    },

    /// Local HTTP server URL could not be built from runtime settings.
    #[error("failed to construct control base URL for {address}: {source}")]
    InvalidControlUrl {
        /// Resolved control address.
        address: SocketAddr,
        /// URL parse error.
        #[source]
        source: url::ParseError,
    },

    /// Background runtime task panicked or was cancelled unexpectedly.
    #[error("forking runtime task failed to join")]
    Join {
        /// Tokio join error.
        #[source]
        source: tokio::task::JoinError,
    },

    /// Runtime server returned an execution error.
    #[error("forking runtime failed: {message}")]
    Runtime {
        /// Detailed error message.
        message: String,
    },
}

/// Typed control client errors for [`crate::ForkingClient`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// Failed to join a relative endpoint path with the configured base URL.
    #[error("failed to resolve endpoint '{path}' against base URL: {source}")]
    UrlJoin {
        /// Relative endpoint path.
        path: String,
        /// URL parse error.
        #[source]
        source: url::ParseError,
    },

    /// Network transport or protocol error from the HTTP client.
    #[error("request to {url} failed")]
    Transport {
        /// Target URL.
        url: url::Url,
        /// Underlying reqwest error.
        #[source]
        source: reqwest::Error,
    },

    /// Endpoint returned a non-success HTTP status.
    #[error("request to {url} failed with status {status}")]
    HttpStatus {
        /// Target URL.
        url: url::Url,
        /// HTTP status code.
        status: reqwest::StatusCode,
        /// Raw response body.
        body: String,
    },

    /// Failed to decode JSON response payload.
    #[error("failed to decode response from {url}")]
    Decode {
        /// Target URL.
        url: url::Url,
        /// Underlying reqwest decode error.
        #[source]
        source: reqwest::Error,
    },

    /// Server returned a successful HTTP response with `success: false`.
    #[error("control endpoint returned failure: {message}")]
    Api {
        /// Server-provided error message.
        message: String,
    },

    /// Server responded with `success: true` but omitted `data`.
    #[error("control endpoint '{endpoint}' returned success without data")]
    MissingData {
        /// Endpoint path.
        endpoint: &'static str,
    },
}
