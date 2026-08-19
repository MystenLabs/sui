// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Selection of the live network to fork.
//!
//! Defines the [`Network`] enum for specifying which Sui network to connect to (mainnet, testnet,
//! devnet, or custom) and provides URL resolution for both GraphQL and gRPC endpoints.

use std::str::FromStr;

use sui_types::supported_protocol_versions::Chain;

/// GraphQL endpoint for Sui mainnet.
pub(crate) const MAINNET_GQL_URL: &str = "https://graphql.mainnet.sui.io/graphql";
/// GraphQL endpoint for Sui testnet.
pub(crate) const TESTNET_GQL_URL: &str = "https://graphql.testnet.sui.io/graphql";
/// GraphQL endpoint for Sui devnet.
pub(crate) const DEVNET_GQL_URL: &str = "https://graphql.devnet.sui.io/graphql";

/// The live Sui network a fork starts from.
#[derive(Clone, Debug)]
pub enum Network {
    /// Sui mainnet
    Mainnet,
    /// Sui testnet
    Testnet,
    /// Sui devnet
    Devnet,
    /// Custom network with a user-provided URL
    Custom(String),
}

impl Network {
    /// Returns the [`Chain`] identifier for this network.
    pub fn chain(&self) -> Chain {
        match self {
            Network::Mainnet => Chain::Mainnet,
            Network::Testnet => Chain::Testnet,
            Network::Devnet => Chain::Unknown,
            Network::Custom(_) => Chain::Unknown,
        }
    }

    /// Returns a human-readable network name.
    pub fn network_name(&self) -> String {
        match self {
            Network::Mainnet => "mainnet".to_string(),
            Network::Testnet => "testnet".to_string(),
            Network::Devnet => "devnet".to_string(),
            Network::Custom(url) => url.clone(),
        }
    }

    /// Returns the GraphQL endpoint URL for this network.
    pub(crate) fn gql_url(&self) -> &str {
        match self {
            Network::Mainnet => MAINNET_GQL_URL,
            Network::Testnet => TESTNET_GQL_URL,
            Network::Devnet => DEVNET_GQL_URL,
            Network::Custom(url) => url.as_str(),
        }
    }
}

impl FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "devnet" => Ok(Network::Devnet),
            _ => Ok(Network::Custom(s.to_string())),
        }
    }
}

/// Round-trips through [`FromStr`], so clap can render defaults from it.
impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => f.write_str("mainnet"),
            Network::Testnet => f.write_str("testnet"),
            Network::Devnet => f.write_str("devnet"),
            Network::Custom(url) => f.write_str(url),
        }
    }
}
