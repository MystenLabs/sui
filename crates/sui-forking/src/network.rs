// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use sui_data_store::{
    Node,
    node::{
        DEVNET_GQL_URL, DEVNET_RPC_URL, MAINNET_GQL_URL, MAINNET_RPC_URL, TESTNET_GQL_URL,
        TESTNET_RPC_URL,
    },
};
use sui_types::supported_protocol_versions::Chain;
use url::Url;

/// Parsed network selection for `sui-forking`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ForkNetwork {
    Mainnet,
    Testnet,
    Devnet,
    Custom(String),
}

impl ForkNetwork {
    /// Parse the CLI `--network` argument.
    ///
    /// Accepted values:
    /// - `mainnet`, `testnet`, `devnet`
    /// - any full `http` or `https` URL (treated as custom GraphQL endpoint)
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(anyhow!("network cannot be empty"));
        }

        match value.to_ascii_lowercase().as_str() {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "devnet" => Ok(Self::Devnet),
            _ => validate_http_url("network", value)
                .map(|()| Self::Custom(value.to_string()))
                .map_err(|e| anyhow!("invalid network value '{value}': {e}")),
        }
    }

    /// Return the protocol config chain override.
    pub(crate) fn protocol_chain(&self) -> Chain {
        match self {
            Self::Mainnet => Chain::Mainnet,
            Self::Testnet => Chain::Testnet,
            Self::Devnet | Self::Custom(_) => Chain::Unknown,
        }
    }

    /// Return the GraphQL endpoint URL used for network reads.
    pub(crate) fn gql_endpoint(&self) -> &str {
        match self {
            Self::Mainnet => MAINNET_GQL_URL,
            Self::Testnet => TESTNET_GQL_URL,
            Self::Devnet => DEVNET_GQL_URL,
            Self::Custom(url) => url.as_str(),
        }
    }

    /// Return the fullnode RPC endpoint for checkpoint/object gRPC reads.
    ///
    /// - `mainnet` / `testnet` / `devnet`: uses defaults unless an override is provided.
    /// - custom GraphQL network: requires `fullnode_url`.
    pub(crate) fn resolve_fullnode_endpoint(&self, fullnode_url: Option<&str>) -> Result<String> {
        let override_url = fullnode_url.and_then(|url| {
            let trimmed = url.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        });

        if let Some(url) = override_url {
            validate_http_url("fullnode URL", url)?;
            return Ok(url.to_string());
        }

        match self {
            Self::Mainnet => Ok(MAINNET_RPC_URL.to_string()),
            Self::Testnet => Ok(TESTNET_RPC_URL.to_string()),
            Self::Devnet => Ok(DEVNET_RPC_URL.to_string()),
            Self::Custom(_) => Err(anyhow!(
                "--fullnode-url is required when --network is a custom GraphQL URL"
            )),
        }
    }

    /// Return the backing data-store node for this network.
    pub(crate) fn node(&self) -> Node {
        match self {
            Self::Mainnet => Node::Mainnet,
            Self::Testnet => Node::Testnet,
            Self::Devnet => Node::Devnet,
            Self::Custom(url) => Node::Custom(url.clone()),
        }
    }

    /// Human-readable label for logging.
    pub(crate) fn display_name(&self) -> &str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Devnet => "devnet",
            Self::Custom(url) => url.as_str(),
        }
    }

    /// Cache path component under `forking/<component>/...`.
    pub(crate) fn cache_path_component(&self) -> String {
        match self {
            Self::Mainnet => "mainnet".to_string(),
            Self::Testnet => "testnet".to_string(),
            Self::Devnet => "devnet".to_string(),
            Self::Custom(url) => {
                let sanitized = sanitize_cache_path_component(url);
                if sanitized.is_empty() {
                    "custom_url".to_string()
                } else {
                    format!("custom_{sanitized}")
                }
            }
        }
    }
}
