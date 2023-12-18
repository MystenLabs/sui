// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{error::Error as SuiGraphQLError, types::big_int::BigInt};
use async_graphql::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use sui_json_rpc::name_service::NameServiceConfig;

use crate::functional_group::FunctionalGroup;

// TODO: calculate proper cost limits
const MAX_QUERY_DEPTH: u32 = 15;
const MAX_QUERY_NODES: u32 = 50;
const MAX_QUERY_PAYLOAD_SIZE: u32 = 2_000;
const MAX_DB_QUERY_COST: u64 = 20_000; // Max DB query cost (normally f64) truncated
const DEFAULT_PAGE_SIZE: u64 = 20; // Default number of elements allowed on a page of a connection
const MAX_PAGE_SIZE: u64 = 50; // Maximum number of elements allowed on a page of a connection
const MAX_TYPE_ARGUMENT_DEPTH: u32 = 16;
const MAX_TYPE_ARGUMENT_WIDTH: u32 = 32;
const MAX_TYPE_NODES: u32 = 256;
const MAX_MOVE_VALUE_DEPTH: u32 = 128;

const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 40_000;

const DEFAULT_IDE_TITLE: &str = "Sui GraphQL IDE";

pub(crate) const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 1_000;

// Default values for the server connection configuration.
pub(crate) const DEFAULT_SERVER_CONNECTION_PORT: u16 = 8000;
pub(crate) const DEFAULT_SERVER_CONNECTION_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_SERVER_DB_URL: &str =
    "postgres://postgres:postgrespw@localhost:5432/sui_indexer_v2";
pub(crate) const DEFAULT_SERVER_DB_POOL_SIZE: u32 = 3;
pub(crate) const DEFAULT_SERVER_PROM_HOST: &str = "0.0.0.0";
pub(crate) const DEFAULT_SERVER_PROM_PORT: u16 = 9184;

/// Configuration on connections for the RPC, passed in as command-line arguments.
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq)]
pub struct ConnectionConfig {
    pub(crate) port: u16,
    pub(crate) host: String,
    pub(crate) db_url: String,
    pub(crate) db_pool_size: u32,
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
    pub max_query_depth: u32,
    #[serde(default)]
    pub max_query_nodes: u32,
    #[serde(default)]
    pub max_query_payload_size: u32,
    #[serde(default)]
    pub max_db_query_cost: u64,
    #[serde(default)]
    pub default_page_size: u64,
    #[serde(default)]
    pub max_page_size: u64,
    #[serde(default)]
    pub request_timeout_ms: u64,
    #[serde(default)]
    pub max_type_argument_depth: u32,
    #[serde(default)]
    pub max_type_argument_width: u32,
    #[serde(default)]
    pub max_type_nodes: u32,
    #[serde(default)]
    pub max_move_value_depth: u32,
}

impl Limits {
    pub fn default_for_simulator_testing() -> Self {
        Self {
            max_query_nodes: 500,
            max_query_depth: 20,
            max_query_payload_size: 5_000,
            ..Self::default()
        }
    }

    /// Extract limits for the package resolver.
    pub fn package_resolver_limits(&self) -> sui_package_resolver::Limits {
        sui_package_resolver::Limits {
            max_type_argument_depth: self.max_type_argument_depth as usize,
            max_type_argument_width: self.max_type_argument_width as usize,
            max_type_nodes: self.max_type_nodes as usize,
            max_move_value_depth: self.max_move_value_depth as usize,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Ide {
    #[serde(default)]
    pub(crate) ide_title: String,
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            ide_title: DEFAULT_IDE_TITLE.to_string(),
        }
    }
}

impl Ide {
    pub fn new(ide_title: Option<String>) -> Self {
        Self {
            ide_title: ide_title.unwrap_or_else(|| DEFAULT_IDE_TITLE.to_string()),
        }
    }
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
        db_pool_size: Option<u32>,
        prom_url: Option<String>,
        prom_port: Option<u16>,
    ) -> Self {
        let default = Self::default();
        Self {
            port: port.unwrap_or(default.port),
            host: host.unwrap_or(default.host),
            db_url: db_url.unwrap_or(default.db_url),
            db_pool_size: db_pool_size.unwrap_or(default.db_pool_size),
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

    pub fn db_pool_size(&self) -> u32 {
        self.db_pool_size
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
    async fn is_enabled(&self, feature: FunctionalGroup) -> bool {
        !self.disabled_features.contains(&feature)
    }

    /// List of all features that are enabled on this GraphQL service.
    async fn enabled_features(&self) -> Vec<FunctionalGroup> {
        FunctionalGroup::all()
            .iter()
            .filter(|g| !self.disabled_features.contains(g))
            .copied()
            .collect()
    }

    /// The maximum depth a GraphQL query can be to be accepted by this service.
    pub async fn max_query_depth(&self) -> u32 {
        self.limits.max_query_depth
    }

    /// The maximum number of nodes (field names) the service will accept in a single query.
    pub async fn max_query_nodes(&self) -> u32 {
        self.limits.max_query_nodes
    }

    /// Maximum estimated cost of a database query used to serve a GraphQL request.  This is
    /// measured in the same units that the database uses in EXPLAIN queries.
    async fn max_db_query_cost(&self) -> BigInt {
        BigInt::from(self.limits.max_db_query_cost)
    }

    /// Default number of elements allowed on a single page of a connection.
    async fn default_page_size(&self) -> u64 {
        self.limits.default_page_size
    }

    /// Maximum number of elements allowed on a single page of a connection.
    async fn max_page_size(&self) -> u64 {
        self.limits.max_page_size
    }

    /// Maximum time in milliseconds that will be spent to serve one request.
    async fn request_timeout_ms(&self) -> u64 {
        self.limits.request_timeout_ms
    }

    /// Maximum length of a query payload string.
    async fn max_query_payload_size(&self) -> u32 {
        self.limits.max_query_payload_size
    }

    /// Maximum nesting allowed in type arguments in Move Types resolved by this service.
    async fn max_type_argument_depth(&self) -> u32 {
        self.limits.max_type_argument_depth
    }

    /// Maximum number of type arguments passed into a generic instantiation of a Move Type resolved
    /// by this service.
    async fn max_type_argument_width(&self) -> u32 {
        self.limits.max_type_argument_width
    }

    /// Maximum number of structs that need to be processed when calculating the layout of a single
    /// Move Type.
    async fn max_type_nodes(&self) -> u32 {
        self.limits.max_type_nodes
    }

    /// Maximum nesting allowed in struct fields when calculating the layout of a single Move Type.
    async fn max_move_value_depth(&self) -> u32 {
        self.limits.max_move_value_depth
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_SERVER_CONNECTION_PORT,
            host: DEFAULT_SERVER_CONNECTION_HOST.to_string(),
            db_url: DEFAULT_SERVER_DB_URL.to_string(),
            db_pool_size: DEFAULT_SERVER_DB_POOL_SIZE,
            prom_url: DEFAULT_SERVER_PROM_HOST.to_string(),
            prom_port: DEFAULT_SERVER_PROM_PORT,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_query_depth: MAX_QUERY_DEPTH,
            max_query_nodes: MAX_QUERY_NODES,
            max_query_payload_size: MAX_QUERY_PAYLOAD_SIZE,
            max_db_query_cost: MAX_DB_QUERY_COST,
            default_page_size: DEFAULT_PAGE_SIZE,
            max_page_size: MAX_PAGE_SIZE,
            request_timeout_ms: DEFAULT_REQUEST_TIMEOUT_MS,
            max_type_argument_depth: MAX_TYPE_ARGUMENT_DEPTH,
            max_type_argument_width: MAX_TYPE_ARGUMENT_WIDTH,
            max_type_nodes: MAX_TYPE_NODES,
            max_move_value_depth: MAX_MOVE_VALUE_DEPTH,
        }
    }
}

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
    #[serde(default)]
    pub(crate) tracing: bool,
    #[serde(default)]
    pub(crate) apollo_tracing: bool,
    #[serde(default)]
    pub(crate) open_telemetry: bool,
}

impl Default for InternalFeatureConfig {
    fn default() -> Self {
        Self {
            query_limits_checker: true,
            feature_gate: true,
            logger: true,
            query_timeout: true,
            metrics: true,
            tracing: false,
            apollo_tracing: false,
            open_telemetry: false,
        }
    }
}

#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq, Default)]
pub struct TxExecFullNodeConfig {
    #[serde(default)]
    pub(crate) node_rpc_url: Option<String>,
}

impl TxExecFullNodeConfig {
    pub fn new(node_rpc_url: Option<String>) -> Self {
        Self { node_rpc_url }
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
    #[serde(default)]
    pub tx_exec_full_node: TxExecFullNodeConfig,
    #[serde(default)]
    pub ide: Ide,
}

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
                max-query-payload-size = 2000
                max-db-query-cost = 50
                default-page-size = 20
                max-page-size = 50
                request-timeout-ms = 27000
                max-type-argument-depth = 32
                max-type-argument-width = 64
                max-type-nodes = 128
                max-move-value-depth = 256
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 100,
                max_query_nodes: 300,
                max_query_payload_size: 2000,
                max_db_query_cost: 50,
                default_page_size: 20,
                max_page_size: 50,
                request_timeout_ms: 27_000,
                max_type_argument_depth: 32,
                max_type_argument_width: 64,
                max_type_nodes: 128,
                max_move_value_depth: 256,
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
                max-query-payload-size = 200
                max-db-query-cost = 20
                default-page-size = 10
                max-page-size = 20
                request-timeout-ms = 30000
                max-type-argument-depth = 32
                max-type-argument-width = 64
                max-type-nodes = 128
                max-move-value-depth = 256

                [experiments]
                test-flag = true
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 42,
                max_query_nodes: 320,
                max_query_payload_size: 200,
                max_db_query_cost: 20,
                default_page_size: 10,
                max_page_size: 20,
                request_timeout_ms: 30_000,
                max_type_argument_depth: 32,
                max_type_argument_width: 64,
                max_type_nodes: 128,
                max_move_value_depth: 256,
            },
            disabled_features: BTreeSet::from([FunctionalGroup::Analytics]),
            experiments: Experiments { test_flag: true },
        };

        assert_eq!(actual, expect);
    }
}
