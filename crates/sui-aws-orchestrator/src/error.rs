// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use reqwest::Url;

#[macro_export(local_inner_macros)]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            return Err($e);
        }
    };
}

pub type SettingsResult<T> = Result<T, SettingsError>;

#[derive(thiserror::Error, Debug)]
pub enum SettingsError {
    #[error("Failed to read settings file '{file:?}': {message}")]
    InvalidSettings { file: String, message: String },

    #[error("Failed to read token file '{file:?}': {message}")]
    InvalidTokenFile { file: String, message: String },

    #[error("Failed to read ssh public key file '{file:?}': {message}")]
    InvalidSshPublicKeyFile { file: String, message: String },

    #[error("Malformed repository url: {0:?}")]
    MalformedRepositoryUrl(Url),
}

pub type CloudProviderResult<T> = Result<T, CloudProviderError>;

#[derive(thiserror::Error, Debug)]
pub enum CloudProviderError {
    #[error("Failed to send server request: {0}")]
    RequestError(String),

    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),

    #[error("Received error status code ({0}): {1}")]
    FailureResponseCode(String, String),

    #[error("SSH key \"{0}\" not found")]
    SshKeyNotFound(String),
}

pub type SshResult<T> = Result<T, SshError>;

#[derive(thiserror::Error, Debug)]
pub enum SshError {
    #[error("Failed to load private key for {address}: {error}")]
    PrivateKeyError {
        address: SocketAddr,
        error: russh_keys::Error,
    },

    #[error("Failed to create ssh session with {address}: {error}")]
    SessionError {
        address: SocketAddr,
        error: russh::Error,
    },

    #[error("Failed to connect to instance {address}: {error}")]
    ConnectionError {
        address: SocketAddr,
        error: russh::Error,
    },

    #[error("Remote execution on {address} returned exit code ({code}): {message}")]
    NonZeroExitCode {
        address: SocketAddr,
        code: u32,
        message: String,
    },
}

pub type MonitorResult<T> = Result<T, MonitorError>;

#[derive(thiserror::Error, Debug)]
pub enum MonitorError {
    #[error(transparent)]
    SshError(#[from] SshError),

    #[error("Failed to start Grafana: {0}")]
    GrafanaError(String),
}

pub type TestbedResult<T> = Result<T, TestbedError>;

#[derive(thiserror::Error, Debug)]
pub enum TestbedError {
    #[error(transparent)]
    SettingsError(#[from] SettingsError),

    #[error(transparent)]
    CloudProviderError(#[from] CloudProviderError),

    #[error(transparent)]
    SshError(#[from] SshError),

    #[error("Not enough instances: missing {0} instances")]
    InsufficientCapacity(usize),

    #[error(transparent)]
    MonitorError(#[from] MonitorError),
}
