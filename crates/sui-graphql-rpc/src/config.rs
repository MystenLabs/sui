// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error as SuiGraphQLError;
use async_graphql::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};
use sui_json_rpc::name_service::NameServiceConfig;

use crate::functional_group::FunctionalGroup;

// TODO: calculate proper cost limits
const MAX_QUERY_DEPTH: u32 = 10;
const MAX_QUERY_NODES: u32 = 100;
const MAX_DB_QUERY_COST: u64 = 50; // Max DB query cost (normally f64) truncated
const MAX_QUERY_VARIABLES: u32 = 50;
const MAX_QUERY_FRAGMENTS: u32 = 50;

/// Configuration on connections for the RPC, passed in as command-line arguments.
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq)]
pub struct ConnectionConfig {
    pub(crate) port: u16,
    pub(crate) host: String,
    pub(crate) db_url: String,
    pub(crate) prom_url: String,
    pub(crate) prom_port: u16,
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

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Copy)]
#[serde(rename_all = "kebab-case")]
pub struct Limits {
    #[serde(default)]
    pub(crate) max_query_depth: u32,
    #[serde(default)]
    pub(crate) max_query_nodes: u32,
    #[serde(default)]
    pub(crate) max_db_query_cost: u64,
    #[serde(default)]
    pub(crate) max_query_variables: u32,
    #[serde(default)]
    pub(crate) max_query_fragments: u32,
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
    pub fn new(
        port: Option<u16>,
        host: Option<String>,
        db_url: Option<String>,
        prom_url: Option<String>,
        prom_port: Option<u16>,
    ) -> Self {
        let default = Self::default();
        Self {
            port: port.unwrap_or(default.port),
            host: host.unwrap_or(default.host),
            db_url: db_url.unwrap_or(default.db_url),
            prom_url: prom_url.unwrap_or(default.prom_url),
            prom_port: prom_port.unwrap_or(default.prom_port),
        }
    }

    pub fn ci_integration_test_cfg() -> Self {
        Self {
            db_url: "postgres://postgres:postgrespw@localhost:5432/sui_indexer_v2".to_string(),
            ..Default::default()
        }
    }

    pub fn db_url(&self) -> String {
        self.db_url.clone()
    }

    pub fn server_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
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
            db_url: "postgres://postgres:postgrespw@localhost:5432/sui_indexer_v2".to_string(),
            prom_url: "0.0.0.0".to_string(),
            prom_port: 9184,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_query_depth: MAX_QUERY_DEPTH,
            max_query_nodes: MAX_QUERY_NODES,
            max_db_query_cost: MAX_DB_QUERY_COST,
            max_query_variables: MAX_QUERY_VARIABLES,
            max_query_fragments: MAX_QUERY_FRAGMENTS,
        }
    }
}

#[allow(dead_code)]
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq)]
pub struct InternalFeatureConfig {
    #[serde(default)]
    pub(crate) query_limits_checker: bool,
    #[serde(default)]
    pub(crate) feature_gate: bool,
    #[serde(default)]
    pub(crate) logger: bool,
    #[serde(default)]
    pub(crate) query_timeout: bool,
    #[serde(default)]
    pub(crate) metrics: bool,
}

impl Default for InternalFeatureConfig {
    fn default() -> Self {
        Self {
            query_limits_checker: true,
            feature_gate: true,
            logger: true,
            query_timeout: true,
            metrics: true,
        }
    }
}

#[derive(Serialize, Clone, Deserialize, Debug, Default)]
pub struct ServerConfig {
    #[serde(default)]
    pub service: ServiceConfig,
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub internal_features: InternalFeatureConfig,
    #[serde(default)]
    pub name_service: NameServiceConfig,
}

#[allow(dead_code)]
impl ServerConfig {
    pub fn from_yaml(path: &str) -> Result<Self, SuiGraphQLError> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            SuiGraphQLError::Internal(format!(
                "Failed to read service cfg yaml file at {}, err: {}",
                path, e
            ))
        })?;
        serde_yaml::from_str::<Self>(&contents).map_err(|e| {
            SuiGraphQLError::Internal(format!(
                "Failed to deserialize service cfg from yaml: {}",
                e
            ))
        })
    }

    pub fn to_yaml(&self) -> Result<String, SuiGraphQLError> {
        serde_yaml::to_string(&self).map_err(|e| {
            SuiGraphQLError::Internal(format!("Failed to create yaml from cfg: {}", e))
        })
    }

    pub fn to_yaml_file(&self, path: PathBuf) -> Result<(), SuiGraphQLError> {
        let config = self.to_yaml()?;
        std::fs::write(path, config).map_err(|e| {
            SuiGraphQLError::Internal(format!("Failed to create yaml from cfg: {}", e))
        })
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
                max-db-query-cost = 50
                max-query-variables = 45
                max-query-fragments = 32
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 100,
                max_query_nodes: 300,
                max_db_query_cost: 50,
                max_query_variables: 45,
                max_query_fragments: 32,
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
                max-db-query-cost = 20
                max-query-variables = 34
                max-query-fragments = 31

                [experiments]
                test-flag = true
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 42,
                max_query_nodes: 320,
                max_db_query_cost: 20,
                max_query_variables: 34,
                max_query_fragments: 31,
            },
            disabled_features: BTreeSet::from([FunctionalGroup::Analytics]),
            experiments: Experiments { test_flag: true },
        };

        assert_eq!(actual, expect);
    }
}
