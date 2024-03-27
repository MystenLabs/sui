// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::functional_group::FunctionalGroup;
use crate::types::big_int::BigInt;
use async_graphql::*;
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Display, time::Duration};
use sui_json_rpc::name_service::NameServiceConfig;

// TODO: calculate proper cost limits

/// These values are set to support TS SDK shim layer queries for json-rpc compatibility.
const MAX_QUERY_NODES: u32 = 300;
const MAX_QUERY_PAYLOAD_SIZE: u32 = 5_000;

const MAX_QUERY_DEPTH: u32 = 20;
const MAX_OUTPUT_NODES: u64 = 100_000; // Maximum number of output nodes allowed in the response
const MAX_DB_QUERY_COST: u64 = 20_000; // Max DB query cost (normally f64) truncated
const DEFAULT_PAGE_SIZE: u64 = 20; // Default number of elements allowed on a page of a connection
const MAX_PAGE_SIZE: u64 = 50; // Maximum number of elements allowed on a page of a connection

/// The following limits reflect the max values set in the ProtocolConfig.
const MAX_TYPE_ARGUMENT_DEPTH: u32 = 16;
const MAX_TYPE_ARGUMENT_WIDTH: u32 = 32;
const MAX_TYPE_NODES: u32 = 256;
const MAX_MOVE_VALUE_DEPTH: u32 = 128;

pub(crate) const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 40_000;

const DEFAULT_IDE_TITLE: &str = "Sui GraphQL IDE";

pub(crate) const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 1_000;

// Default values for the server connection configuration.
pub(crate) const DEFAULT_SERVER_CONNECTION_PORT: u16 = 8000;
pub(crate) const DEFAULT_SERVER_CONNECTION_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_SERVER_DB_URL: &str =
    "postgres://postgres:postgrespw@localhost:5432/sui_indexer";
pub(crate) const DEFAULT_SERVER_DB_POOL_SIZE: u32 = 3;
pub(crate) const DEFAULT_SERVER_PROM_HOST: &str = "0.0.0.0";
pub(crate) const DEFAULT_SERVER_PROM_PORT: u16 = 9184;
pub(crate) const DEFAULT_WATERMARK_UPDATE_MS: u64 = 500;

/// The combination of all configurations for the GraphQL service.
#[derive(Serialize, Clone, Deserialize, Debug, Default)]
pub struct ServerConfig {
    #[serde(default)]
    pub service: ServiceConfig,
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub internal_features: InternalFeatureConfig,
    #[serde(default)]
    pub tx_exec_full_node: TxExecFullNodeConfig,
    #[serde(default)]
    pub ide: Ide,
}

/// Configuration for connections for the RPC, passed in as command-line arguments. This configures
/// specific connections between this service and other services, and might differ from instance to
/// instance of the GraphQL service.
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq)]
pub struct ConnectionConfig {
    /// Port to bind the server to
    pub(crate) port: u16,
    /// Host to bind the server to
    pub(crate) host: String,
    pub(crate) db_url: String,
    pub(crate) db_pool_size: u32,
    pub(crate) prom_url: String,
    pub(crate) prom_port: u16,
}

/// Configuration on features supported by the GraphQL service, passed in a TOML-based file. These
/// configurations are shared across fleets of the service, i.e. all testnet services will have the
/// same `ServiceConfig`.
#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[serde(default)]
    pub(crate) limits: Limits,

    #[serde(default)]
    pub(crate) disabled_features: BTreeSet<FunctionalGroup>,

    #[serde(default)]
    pub(crate) experiments: Experiments,

    #[serde(default)]
    pub(crate) name_service: NameServiceConfig,

    #[serde(default)]
    pub(crate) background_tasks: BackgroundTasksConfig,

    #[serde(default)]
    pub(crate) zklogin: ZkLoginConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Copy)]
#[serde(rename_all = "kebab-case")]
pub struct Limits {
    #[serde(default)]
    pub max_query_depth: u32,
    #[serde(default)]
    pub max_query_nodes: u32,
    #[serde(default)]
    pub max_output_nodes: u64,
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

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Copy)]
#[serde(rename_all = "kebab-case")]
pub struct BackgroundTasksConfig {
    #[serde(default)]
    pub watermark_update_ms: u64,
}

/// The Version of the service. `year.month` represents the major release.
/// New `patch` versions represent backwards compatible fixes for their major release.
/// The `full` version is `year.month.patch-sha`.
#[derive(Copy, Clone, Debug)]
pub struct Version {
    /// The year of this release.
    pub year: &'static str,
    /// The month of this release.
    pub month: &'static str,
    /// The patch is a positive number incremented for every compatible release on top of the major.month release.
    pub patch: &'static str,
    /// The commit sha for this release.
    pub sha: &'static str,
    /// The full version string.
    /// Note that this extra field is used only for the uptime_metric function which requries a
    /// &'static str.
    pub full: &'static str,
}

impl Version {
    /// Use for testing when you need the Version obj and a year.month &str
    pub fn for_testing() -> Self {
        Self {
            year: env!("CARGO_PKG_VERSION_MAJOR"),
            month: env!("CARGO_PKG_VERSION_MINOR"),
            patch: env!("CARGO_PKG_VERSION_PATCH"),
            sha: "testing-no-sha",
            // note that this full field is needed for metrics but not for testing
            full: const_str::concat!(
                env!("CARGO_PKG_VERSION_MAJOR"),
                ".",
                env!("CARGO_PKG_VERSION_MINOR"),
                ".",
                env!("CARGO_PKG_VERSION_PATCH"),
                "-testing-no-sha"
            ),
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Ide {
    #[serde(default)]
    pub(crate) ide_title: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Experiments {
    // Add experimental flags here, to provide access to them through-out the GraphQL
    // implementation.
    #[cfg(test)]
    test_flag: bool,
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

#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq, Default)]
pub struct TxExecFullNodeConfig {
    #[serde(default)]
    pub(crate) node_rpc_url: Option<String>,
}

#[derive(Serialize, Clone, Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ZkLoginConfig {
    pub env: ZkLoginEnv,
}

/// The enabled features and service limits configured by the server.
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

    /// The maximum number of output nodes in a GraphQL response.
    ///
    /// Non-connection nodes have a count of 1, while connection nodes are counted as
    /// the specified 'first' or 'last' number of items, or the default_page_size
    /// as set by the server if those arguments are not set.
    ///
    /// Counts accumulate multiplicatively down the query tree. For example, if a query starts
    /// with a connection of first: 10 and has a field to a connection with last: 20, the count
    /// at the second level would be 200 nodes. This is then summed to the count of 10 nodes
    /// at the first level, for a total of 210 nodes.
    pub async fn max_output_nodes(&self) -> u64 {
        self.limits.max_output_nodes
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

impl TxExecFullNodeConfig {
    pub fn new(node_rpc_url: Option<String>) -> Self {
        Self { node_rpc_url }
    }
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
            db_url: DEFAULT_SERVER_DB_URL.to_string(),
            ..Default::default()
        }
    }

    pub fn ci_integration_test_cfg_with_db_name(
        db_name: String,
        port: u16,
        prom_port: u16,
    ) -> Self {
        Self {
            db_url: format!("postgres://postgres:postgrespw@localhost:5432/{}", db_name),
            port,
            prom_port,
            ..Default::default()
        }
    }

    pub fn db_name(&self) -> String {
        self.db_url.split('/').last().unwrap().to_string()
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

    pub fn test_defaults() -> Self {
        Self {
            background_tasks: BackgroundTasksConfig::test_defaults(),
            zklogin: ZkLoginConfig {
                env: ZkLoginEnv::Test,
            },
            ..Default::default()
        }
    }
}

impl Limits {
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

impl Ide {
    pub fn new(ide_title: Option<String>) -> Self {
        Self {
            ide_title: ide_title.unwrap_or_else(|| DEFAULT_IDE_TITLE.to_string()),
        }
    }
}

impl BackgroundTasksConfig {
    pub fn test_defaults() -> Self {
        Self {
            watermark_update_ms: 100, // Set to 100ms for testing
        }
    }
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            ide_title: DEFAULT_IDE_TITLE.to_string(),
        }
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
            max_output_nodes: MAX_OUTPUT_NODES,
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

impl Default for BackgroundTasksConfig {
    fn default() -> Self {
        Self {
            watermark_update_ms: DEFAULT_WATERMARK_UPDATE_MS,
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
                max-output-nodes = 200000
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
                max_output_nodes: 200000,
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
            disabled_features: BTreeSet::from([G::Coins, G::NameService]),
            ..Default::default()
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
                max-output-nodes = 200000
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
                max_output_nodes: 200000,
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
            ..Default::default()
        };

        assert_eq!(actual, expect);
    }
}
