// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::functional_group::FunctionalGroup;

/// Configuration on connections for the RPC, passed in as command-line arguments.
pub struct ConnectionConfig {
    pub(crate) port: u16,
    pub(crate) host: String,
    pub(crate) rpc_url: String,
}

/// Configuration on features supported by the RPC, passed in a TOML-based file.
#[derive(Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[serde(default)]
    pub(crate) limits: Limits,

    #[serde(default)]
    pub(crate) disabled_features: BTreeSet<FunctionalGroup>,

    #[serde(default)]
    pub(crate) experiments: Experiments,
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Limits {
    #[serde(default)]
    pub(crate) max_query_depth: usize,
}

#[derive(Deserialize, Debug, Eq, PartialEq, Default)]
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
            max_query_depth: 10,
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
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 100,
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
                  "name-server",
                ]
            "#,
        )
        .unwrap();

        use FunctionalGroup as G;
        let expect = ServiceConfig {
            limits: Limits::default(),
            disabled_features: BTreeSet::from([G::Coins, G::NameServer]),
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

                [experiments]
                test-flag = true
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 42,
            },
            disabled_features: BTreeSet::from([FunctionalGroup::Analytics]),
            experiments: Experiments { test_flag: true },
        };

        assert_eq!(actual, expect);
    }
}
