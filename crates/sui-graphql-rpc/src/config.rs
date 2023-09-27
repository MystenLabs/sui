// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use async_graphql::*;
use serde::{Deserialize, Serialize};

use crate::functional_group::FunctionalGroup;

const MAX_QUERY_DEPTH: u32 = 10;
const MAX_QUERY_NODES: u32 = 100;

/// Configuration on connections for the RPC, passed in as command-line arguments.
pub struct ConnectionConfig {
    pub(crate) port: u16,
    pub(crate) host: String,
    pub(crate) rpc_url: String,
}

/// Configuration on features supported by the RPC, passed in a TOML-based file.
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[serde(default)]
    pub(crate) limits: Limits,

    #[serde(default)]
    pub(crate) disabled_features: BTreeSet<FunctionalGroup>,

    #[serde(default)]
    pub(crate) experiments: Experiments,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Limits {
    #[serde(default)]
    pub(crate) max_query_depth: u32,
    #[serde(default)]
    pub(crate) max_query_nodes: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Experiments {
    // Add experimental flags here, to provide access to them through-out the GraphQL
    // implementation.
    #[cfg(test)]
    test_flag: bool,
}

impl ConnectionConfig {
    pub fn new(port: Option<u16>, host: Option<String>, rpc_url: Option<String>) -> Self {
        let default = Self::default();
        Self {
            port: port.unwrap_or(default.port),
            host: host.unwrap_or(default.host),
            rpc_url: rpc_url.unwrap_or(default.rpc_url),
        }
    }
}

impl ServiceConfig {
    pub fn read(contents: &str) -> Result<Self, toml::de::Error> {
        toml::de::from_str::<Self>(contents)
    }
}

#[Object]
impl ServiceConfig {
    /// Check whether `feature` is enabled on this GraphQL service.
    async fn is_enabled(&self, feature: FunctionalGroup) -> Result<bool> {
        Ok(!self.disabled_features.contains(&feature))
    }

    /// List of all features that are enabled on this GraphQL service.
    async fn enabled_features(&self) -> Result<Vec<FunctionalGroup>> {
        Ok(FunctionalGroup::all()
            .iter()
            .filter(|g| !self.disabled_features.contains(g))
            .copied()
            .collect())
    }

    /// The maximum depth a GraphQL query can be to be accepted by this service.
    async fn max_query_depth(&self) -> Result<u32> {
        Ok(self.limits.max_query_depth)
    }

    /// The maximum number of nodes (field names) the service will accept in a single query.
    async fn max_query_nodes(&self) -> Result<u32> {
        Ok(self.limits.max_query_nodes)
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            port: 8000,
            host: "127.0.0.1".to_string(),
            rpc_url: "https://fullnode.testnet.sui.io:443/".to_string(),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_query_depth: MAX_QUERY_DEPTH,
            max_query_nodes: MAX_QUERY_NODES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_empty_service_config() {
        let actual = ServiceConfig::read("").unwrap();
        let expect = ServiceConfig::default();
        assert_eq!(actual, expect);
    }

    #[test]
    fn test_read_limits_in_service_config() {
        let actual = ServiceConfig::read(
            r#" [limits]
                max-query-depth = 100
                max-query-nodes = 300
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 100,
                max_query_nodes: 300,
            },
            ..Default::default()
        };

        assert_eq!(actual, expect)
    }

    #[test]
    fn test_read_enabled_features_in_service_config() {
        let actual = ServiceConfig::read(
            r#" disabled-features = [
                  "coins",
                  "name-service",
                ]
            "#,
        )
        .unwrap();

        use FunctionalGroup as G;
        let expect = ServiceConfig {
            limits: Limits::default(),
            disabled_features: BTreeSet::from([G::Coins, G::NameService]),
            experiments: Experiments::default(),
        };

        assert_eq!(actual, expect)
    }

    #[test]
    fn test_read_experiments_in_service_config() {
        let actual = ServiceConfig::read(
            r#" [experiments]
                test-flag = true
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            experiments: Experiments { test_flag: true },
            ..Default::default()
        };

        assert_eq!(actual, expect)
    }

    #[test]
    fn test_read_everything_in_service_config() {
        let actual = ServiceConfig::read(
            r#" disabled-features = ["analytics"]

                [limits]
                max-query-depth = 42
                max-query-nodes = 320

                [experiments]
                test-flag = true
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 42,
                max_query_nodes: 320,
            },
            disabled_features: BTreeSet::from([FunctionalGroup::Analytics]),
            experiments: Experiments { test_flag: true },
        };

        assert_eq!(actual, expect);
    }
}
