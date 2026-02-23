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

    /// Cache namespace segment under `forking/<namespace>/...`.
    pub(crate) fn cache_namespace(&self) -> String {
        match self {
            Self::Mainnet => "mainnet".to_string(),
            Self::Testnet => "testnet".to_string(),
            Self::Devnet => "devnet".to_string(),
            Self::Custom(url) => {
                let sanitized = sanitize_custom_namespace(url);
                if sanitized.is_empty() {
                    "custom_url".to_string()
                } else {
                    format!("custom_{sanitized}")
                }
            }
        }
    }
}

fn sanitize_custom_namespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_is_underscore = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_is_underscore = false;
        } else if !prev_is_underscore {
            out.push('_');
            prev_is_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn validate_http_url(field_name: &str, value: &str) -> Result<()> {
    let parsed = Url::parse(value)
        .map_err(|e| anyhow!("expected mainnet/testnet/devnet or a full http(s) URL ({e})"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(anyhow!(
            "unsupported URL scheme '{scheme}' for {field_name}; expected http or https"
        )),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{ForkNetwork, sanitize_custom_namespace};

    #[test]
    fn parses_known_network_keywords() {
        assert_eq!(ForkNetwork::parse("mainnet").unwrap(), ForkNetwork::Mainnet);
        assert_eq!(ForkNetwork::parse("testnet").unwrap(), ForkNetwork::Testnet);
        assert_eq!(ForkNetwork::parse("devnet").unwrap(), ForkNetwork::Devnet);
    }

    #[test]
    fn parses_custom_graphql_url_without_rewriting() {
        let url = "https://example.com/custom/graphql";
        let network = ForkNetwork::parse(url).unwrap();
        assert_eq!(network.gql_endpoint(), url);
    }

    #[test]
    fn rejects_invalid_non_url_custom_values() {
        let err = ForkNetwork::parse("not-a-network").unwrap_err().to_string();
        assert!(err.contains("expected mainnet/testnet/devnet"));
    }

    #[test]
    fn rejects_non_http_scheme_custom_values() {
        let err = ForkNetwork::parse("ws://example.com/graphql")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsupported URL scheme"));
    }

    #[test]
    fn resolves_default_fullnode_url_for_known_networks() {
        assert_eq!(
            ForkNetwork::Mainnet
                .resolve_fullnode_endpoint(None)
                .unwrap(),
            "https://fullnode.mainnet.sui.io:443"
        );
        assert_eq!(
            ForkNetwork::Testnet
                .resolve_fullnode_endpoint(None)
                .unwrap(),
            "https://fullnode.testnet.sui.io:443"
        );
        assert_eq!(
            ForkNetwork::Devnet.resolve_fullnode_endpoint(None).unwrap(),
            "https://fullnode.devnet.sui.io:443"
        );
    }

    #[test]
    fn requires_fullnode_url_for_custom_network() {
        let network = ForkNetwork::parse("https://example.com/graphql").unwrap();
        let err = network
            .resolve_fullnode_endpoint(None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--fullnode-url is required"));
    }

    #[test]
    fn accepts_override_fullnode_url() {
        let network = ForkNetwork::Mainnet;
        assert_eq!(
            network
                .resolve_fullnode_endpoint(Some("https://my-rpc.example.com:443"))
                .unwrap(),
            "https://my-rpc.example.com:443"
        );
    }

    #[test]
    fn cache_namespace_for_known_networks_is_stable() {
        assert_eq!(ForkNetwork::Mainnet.cache_namespace(), "mainnet");
        assert_eq!(ForkNetwork::Testnet.cache_namespace(), "testnet");
        assert_eq!(ForkNetwork::Devnet.cache_namespace(), "devnet");
    }

    #[test]
    fn custom_cache_namespace_is_sanitized_and_deterministic() {
        let network =
            ForkNetwork::parse("HTTPS://GraphQL.DevNet.Sui.IO/graphql?foo=bar&&baz=1").unwrap();
        assert_eq!(
            network.cache_namespace(),
            "custom_https_graphql_devnet_sui_io_graphql_foo_bar_baz_1"
        );
    }

    proptest! {
        #[test]
        fn sanitization_output_is_valid(input in ".*") {
            let output = sanitize_custom_namespace(&input);
            prop_assert!(output.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'));
            prop_assert!(!output.starts_with('_'));
            prop_assert!(!output.ends_with('_'));
            prop_assert!(!output.contains("__"));
        }

        #[test]
        fn sanitization_is_idempotent(input in ".*") {
            let once = sanitize_custom_namespace(&input);
            let twice = sanitize_custom_namespace(&once);
            prop_assert_eq!(once, twice);
        }
    }
}
