// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Configuration types for starting `sui-forking` programmatically.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use sui_types::base_types::{ObjectID, SuiAddress};
use url::Url;

use crate::api::error::ConfigError;

const DEFAULT_RPC_PORT: u16 = 9000;
const DEFAULT_SERVER_PORT: u16 = 9001;

/// Supported source network selection for forking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForkingNetwork {
    /// Sui mainnet.
    Mainnet,
    /// Sui testnet.
    Testnet,
    /// Sui devnet.
    Devnet,
    /// Custom GraphQL endpoint URL.
    Custom(Url),
}

impl ForkingNetwork {
    /// Parse a network selector from CLI-style input.
    ///
    /// Accepted values:
    /// - `mainnet`
    /// - `testnet`
    /// - `devnet`
    /// - any full `http(s)` URL
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the input is empty, malformed, or uses a non-HTTP scheme.
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        let trimmed = value.trim();
        let parsed = crate::network::ForkNetwork::parse(trimmed).map_err(|err| {
            let message = err.to_string();
            if let Some(scheme) = parse_scheme_from_error(&message) {
                return ConfigError::InvalidUrlScheme {
                    field: "network",
                    scheme,
                };
            }
            ConfigError::InvalidNetwork {
                value: value.to_string(),
                reason: message,
            }
        })?;

        match parsed {
            crate::network::ForkNetwork::Mainnet => Ok(Self::Mainnet),
            crate::network::ForkNetwork::Testnet => Ok(Self::Testnet),
            crate::network::ForkNetwork::Devnet => Ok(Self::Devnet),
            crate::network::ForkNetwork::Custom(url) => {
                let parsed_url = Url::parse(&url).map_err(|err| ConfigError::InvalidNetwork {
                    value: url,
                    reason: err.to_string(),
                })?;
                Ok(Self::Custom(parsed_url))
            }
        }
    }

    pub(crate) fn to_internal(&self) -> crate::network::ForkNetwork {
        match self {
            Self::Mainnet => crate::network::ForkNetwork::Mainnet,
            Self::Testnet => crate::network::ForkNetwork::Testnet,
            Self::Devnet => crate::network::ForkNetwork::Devnet,
            Self::Custom(url) => crate::network::ForkNetwork::Custom(url.to_string()),
        }
    }
}

impl FromStr for ForkingNetwork {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl std::fmt::Display for ForkingNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Testnet => write!(f, "testnet"),
            Self::Devnet => write!(f, "devnet"),
            Self::Custom(url) => write!(f, "{url}"),
        }
    }
}

/// Startup seeding mode for prefetching objects at boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupSeeding {
    /// Do not prefetch startup objects.
    None,
    /// Prefetch objects owned by these addresses.
    Accounts(Vec<SuiAddress>),
    /// Prefetch these explicit object IDs.
    Objects(Vec<ObjectID>),
}

impl StartupSeeding {
    pub(crate) fn into_internal(self) -> crate::seeds::StartupSeeds {
        match self {
            Self::None => crate::seeds::StartupSeeds::default(),
            Self::Accounts(accounts) => crate::seeds::StartupSeeds {
                accounts,
                objects: vec![],
            },
            Self::Objects(objects) => crate::seeds::StartupSeeds {
                accounts: vec![],
                objects,
            },
        }
    }
}

/// Immutable node configuration used to start a programmatic forking runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkingNodeConfig {
    pub(crate) network: ForkingNetwork,
    pub(crate) checkpoint: Option<u64>,
    pub(crate) fullnode_url: Option<Url>,
    pub(crate) host: IpAddr,
    pub(crate) server_port: u16,
    pub(crate) rpc_port: u16,
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) startup_seeding: StartupSeeding,
}

impl ForkingNodeConfig {
    /// Create a new builder with defaults matching CLI behavior.
    pub fn builder() -> ForkingNodeConfigBuilder {
        ForkingNodeConfigBuilder::default()
    }

    /// Returns the selected network.
    pub fn network(&self) -> &ForkingNetwork {
        &self.network
    }

    /// Returns the optional checkpoint override.
    pub fn checkpoint(&self) -> Option<u64> {
        self.checkpoint
    }

    /// Returns the optional fullnode URL override.
    pub fn fullnode_url(&self) -> Option<&Url> {
        self.fullnode_url.as_ref()
    }

    /// Returns the bind host for the control HTTP server.
    pub fn host(&self) -> IpAddr {
        self.host
    }

    /// Returns the control HTTP server port.
    pub fn server_port(&self) -> u16 {
        self.server_port
    }

    /// Returns the gRPC RPC port.
    pub fn rpc_port(&self) -> u16 {
        self.rpc_port
    }

    /// Returns the optional data directory root.
    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }

    /// Returns configured startup seeding.
    pub fn startup_seeding(&self) -> &StartupSeeding {
        &self.startup_seeding
    }
}

/// Builder for [`ForkingNodeConfig`].
#[derive(Clone, Debug)]
#[must_use]
pub struct ForkingNodeConfigBuilder {
    network: ForkingNetwork,
    checkpoint: Option<u64>,
    fullnode_url: Option<Url>,
    host: IpAddr,
    server_port: u16,
    rpc_port: u16,
    data_dir: Option<PathBuf>,
    startup_seeding: StartupSeeding,
}

impl Default for ForkingNodeConfigBuilder {
    fn default() -> Self {
        Self {
            network: ForkingNetwork::Mainnet,
            checkpoint: None,
            fullnode_url: None,
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            server_port: DEFAULT_SERVER_PORT,
            rpc_port: DEFAULT_RPC_PORT,
            data_dir: None,
            startup_seeding: StartupSeeding::None,
        }
    }
}

impl ForkingNodeConfigBuilder {
    /// Set the source network.
    pub fn network(mut self, network: ForkingNetwork) -> Self {
        self.network = network;
        self
    }

    /// Set the optional starting checkpoint.
    pub fn checkpoint(mut self, checkpoint: u64) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    /// Set the fullnode URL override.
    pub fn fullnode_url(mut self, url: Url) -> Self {
        self.fullnode_url = Some(url);
        self
    }

    /// Set the bind host for the HTTP control server.
    pub fn host(mut self, host: IpAddr) -> Self {
        self.host = host;
        self
    }

    /// Set the gRPC RPC server port.
    pub fn rpc_port(mut self, port: u16) -> Self {
        self.rpc_port = port;
        self
    }

    /// Set the HTTP control server port.
    pub fn server_port(mut self, port: u16) -> Self {
        self.server_port = port;
        self
    }

    /// Set the optional local data directory root.
    pub fn data_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(path.into());
        self
    }

    /// Set startup object seeding behavior.
    pub fn startup_seeding(mut self, seeding: StartupSeeding) -> Self {
        self.startup_seeding = seeding;
        self
    }

    /// Build a validated [`ForkingNodeConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when any field combination is invalid.
    pub fn build(self) -> Result<ForkingNodeConfig, ConfigError> {
        if self.rpc_port == 0 {
            return Err(ConfigError::InvalidPort { field: "rpc_port" });
        }
        if self.server_port == 0 {
            return Err(ConfigError::InvalidPort {
                field: "server_port",
            });
        }

        if let Some(path) = &self.data_dir
            && path.as_os_str().is_empty()
        {
            return Err(ConfigError::EmptyDataDir);
        }

        if let Some(fullnode_url) = &self.fullnode_url {
            validate_http_or_https("fullnode_url", fullnode_url)?;
        }

        if let ForkingNetwork::Custom(url) = &self.network {
            validate_http_or_https("network", url)?;
            if self.fullnode_url.is_none() {
                return Err(ConfigError::MissingFullnodeUrlForCustomNetwork);
            }
        }

        Ok(ForkingNodeConfig {
            network: self.network,
            checkpoint: self.checkpoint,
            fullnode_url: self.fullnode_url,
            host: self.host,
            server_port: self.server_port,
            rpc_port: self.rpc_port,
            data_dir: self.data_dir,
            startup_seeding: self.startup_seeding,
        })
    }
}

fn validate_http_or_https(field: &'static str, url: &Url) -> Result<(), ConfigError> {
    match url.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(ConfigError::InvalidUrlScheme {
            field,
            scheme: scheme.to_string(),
        }),
    }
}

fn parse_scheme_from_error(message: &str) -> Option<String> {
    let prefix = "unsupported URL scheme '";
    let start = message.find(prefix)?;
    let after_prefix = &message[(start + prefix.len())..];
    let end = after_prefix.find('\'')?;
    Some(after_prefix[..end].to_string())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn builder_defaults_match_cli_defaults() {
        let config = ForkingNodeConfig::builder()
            .build()
            .expect("valid defaults");
        assert_eq!(config.network(), &ForkingNetwork::Mainnet);
        assert_eq!(config.host(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.rpc_port(), 9000);
        assert_eq!(config.server_port(), 9001);
        assert!(config.checkpoint().is_none());
        assert!(matches!(config.startup_seeding(), StartupSeeding::None));
    }

    #[test]
    fn rejects_zero_ports() {
        let rpc_err = ForkingNodeConfig::builder()
            .rpc_port(0)
            .build()
            .expect_err("rpc_port=0 must fail");
        assert!(matches!(
            rpc_err,
            ConfigError::InvalidPort { field: "rpc_port" }
        ));

        let http_err = ForkingNodeConfig::builder()
            .server_port(0)
            .build()
            .expect_err("server_port=0 must fail");
        assert!(matches!(
            http_err,
            ConfigError::InvalidPort {
                field: "server_port"
            }
        ));
    }

    #[test]
    fn rejects_custom_network_without_fullnode() {
        let custom_network = ForkingNetwork::Custom(
            Url::parse("https://example.com/graphql").expect("custom network URL"),
        );
        let err = ForkingNodeConfig::builder()
            .network(custom_network)
            .build()
            .expect_err("missing fullnode_url must fail");
        assert!(matches!(
            err,
            ConfigError::MissingFullnodeUrlForCustomNetwork
        ));
    }

    #[test]
    fn accepts_custom_network_with_fullnode() {
        let custom_network = ForkingNetwork::Custom(
            Url::parse("https://example.com/graphql").expect("custom network URL"),
        );
        let fullnode_url = Url::parse("https://example.com/fullnode").expect("fullnode URL");
        let config = ForkingNodeConfig::builder()
            .network(custom_network)
            .fullnode_url(fullnode_url)
            .build()
            .expect("valid custom network config");
        assert!(matches!(config.network(), ForkingNetwork::Custom(_)));
    }

    #[test]
    fn parses_keyword_networks() {
        assert_eq!(
            ForkingNetwork::parse("mainnet").expect("mainnet parse"),
            ForkingNetwork::Mainnet
        );
        assert_eq!(
            ForkingNetwork::parse("testnet").expect("testnet parse"),
            ForkingNetwork::Testnet
        );
        assert_eq!(
            ForkingNetwork::parse("devnet").expect("devnet parse"),
            ForkingNetwork::Devnet
        );
    }

    #[test]
    fn parses_custom_url_network() {
        let parsed = ForkingNetwork::parse("https://my-network.example.com/graphql")
            .expect("custom URL parse");
        assert!(matches!(parsed, ForkingNetwork::Custom(_)));
    }

    #[test]
    fn rejects_invalid_network_string() {
        let err = ForkingNetwork::parse("not-a-network").expect_err("invalid network must fail");
        assert!(matches!(err, ConfigError::InvalidNetwork { .. }));
    }

    #[test]
    fn rejects_non_http_network_scheme() {
        let err = ForkingNetwork::parse("ws://example.com/graphql")
            .expect_err("non-http network scheme must fail");
        assert!(matches!(
            err,
            ConfigError::InvalidUrlScheme {
                field: "network",
                ..
            }
        ));
    }

    #[test]
    fn rejects_non_http_fullnode_scheme() {
        let fullnode_url = Url::parse("ws://example.com:9000").expect("url parse");
        let err = ForkingNodeConfig::builder()
            .fullnode_url(fullnode_url)
            .build()
            .expect_err("non-http fullnode URL must fail");
        assert!(matches!(err, ConfigError::InvalidUrlScheme { .. }));
    }

    proptest! {
        #[test]
        fn keyword_network_parse_is_deterministic(
            keyword in prop_oneof![Just("mainnet"), Just("testnet"), Just("devnet")]
        ) {
            let first = ForkingNetwork::parse(keyword).expect("valid keyword");
            let second = ForkingNetwork::parse(keyword).expect("valid keyword");

            prop_assert_eq!(&first, &second);
            prop_assert_eq!(first.to_string(), keyword);
        }

        #[test]
        fn builder_port_invariants_hold_for_arbitrary_ports(
            rpc_port in any::<u16>(),
            server_port in any::<u16>()
        ) {
            let result = ForkingNodeConfig::builder()
                .rpc_port(rpc_port)
                .server_port(server_port)
                .build();

            if rpc_port == 0 || server_port == 0 {
                prop_assert!(result.is_err());
            } else {
                prop_assert!(result.is_ok());
            }
        }
    }
}
