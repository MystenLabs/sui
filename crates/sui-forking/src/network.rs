// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use url::Url;

/// Parsed network selection for `sui-forking`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
    Custom(String),
}

impl Network {
    /// Parse a (future) CLI `--network` argument.
    ///
    /// Accepted values:
    /// - `mainnet`, `testnet`, `devnet`
    /// - any full `http` or `https` URL (treated as custom GraphQL endpoint)
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(anyhow!("network cannot be empty"));
        }

        match value.to_ascii_lowercase().as_str() {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "devnet" => Ok(Self::Devnet),
            _ => Self::validate_http_url("network", value)
                .map(|()| Self::Custom(value.to_string()))
                .map_err(|e| anyhow!("invalid network value '{value}': {e}")),
        }
    }

    /// Return the GraphQL endpoint URL used for network reads.
    pub fn gql_endpoint(&self) -> &str {
        match self {
            Self::Mainnet => sui_graphql::Client::MAINNET,
            Self::Testnet => sui_graphql::Client::TESTNET,
            Self::Devnet => sui_graphql::Client::DEVNET,
            Self::Custom(url) => url.as_str(),
        }
    }

    /// Human-readable label for logging.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Devnet => "devnet",
            Self::Custom(url) => url.as_str(),
        }
    }

    fn validate_http_url(name: &str, value: &str) -> Result<()> {
        let url = Url::parse(value).map_err(|e| anyhow!("invalid URL for {name}: {e}"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(anyhow!("URL scheme must be http or https"));
        }
        Ok(())
    }
}
