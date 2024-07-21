// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::functional_group::FunctionalGroup;
use async_graphql::*;
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Display, time::Duration};
use sui_graphql_config::GraphQLConfig;
use sui_json_rpc::name_service::NameServiceConfig;

pub(crate) const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 1_000;

/// The combination of all configurations for the GraphQL service.
#[GraphQLConfig]
#[derive(Default)]
pub struct ServerConfig {
    pub service: ServiceConfig,
    pub connection: ConnectionConfig,
    pub internal_features: InternalFeatureConfig,
    pub tx_exec_full_node: TxExecFullNodeConfig,
    pub ide: Ide,
}

/// Configuration for connections for the RPC, passed in as command-line arguments. This configures
/// specific connections between this service and other services, and might differ from instance to
/// instance of the GraphQL service.
#[GraphQLConfig]
#[derive(Clone, Eq, PartialEq)]
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
#[GraphQLConfig]
#[derive(Default)]
pub struct ServiceConfig {
    pub(crate) versions: Versions,
    pub(crate) limits: Limits,
    pub(crate) disabled_features: BTreeSet<FunctionalGroup>,
    pub(crate) experiments: Experiments,
    pub(crate) name_service: NameServiceConfig,
    pub(crate) background_tasks: BackgroundTasksConfig,
    pub(crate) zklogin: ZkLoginConfig,
}

#[GraphQLConfig]
pub struct Versions {
    versions: Vec<String>,
}

#[GraphQLConfig]
pub struct Limits {
    /// Maximum depth of nodes in the requests.
    pub max_query_depth: u32,
    /// Maximum number of nodes in the requests.
    pub max_query_nodes: u32,
    /// Maximum number of output nodes allowed in the response.
    pub max_output_nodes: u32,
    /// Maximum size (in bytes) of a GraphQL request.
    pub max_query_payload_size: u32,
    /// Queries whose EXPLAIN cost are more than this will be logged. Given in the units used by the
    /// database (where 1.0 is roughly the cost of a sequential page access).
    pub max_db_query_cost: u32,
    /// Paginated queries will return this many elements if a page size is not provided.
    pub default_page_size: u32,
    /// Paginated queries can return at most this many elements.
    pub max_page_size: u32,
    /// Time (in milliseconds) to wait for a transaction to be executed and the results returned
    /// from GraphQL. If the transaction takes longer than this time to execute, the request will
    /// return a timeout error, but the transaction may continue executing.
    pub mutation_timeout_ms: u32,
    /// Time (in milliseconds) to wait for a read request from the GraphQL service. Requests that
    /// take longer than this time to return a result will return a timeout error.
    pub request_timeout_ms: u32,
    /// Maximum amount of nesting among type arguments (type arguments nest when a type argument is
    /// itself generic and has arguments).
    pub max_type_argument_depth: u32,
    /// Maximum number of type parameters a type can have.
    pub max_type_argument_width: u32,
    /// Maximum size of a fully qualified type.
    pub max_type_nodes: u32,
    /// Maximum deph of a move value.
    pub max_move_value_depth: u32,
}

#[GraphQLConfig]
#[derive(Copy)]
pub struct BackgroundTasksConfig {
    /// How often the watermark task checks the indexer database to update the checkpoint and epoch
    /// watermarks.
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

#[GraphQLConfig]
pub struct Ide {
    pub(crate) ide_title: String,
}

#[GraphQLConfig]
#[derive(Default)]
pub struct Experiments {
    // Add experimental flags here, to provide access to them through-out the GraphQL
    // implementation.
    #[cfg(test)]
    test_flag: bool,
}

#[GraphQLConfig]
pub struct InternalFeatureConfig {
    pub(crate) query_limits_checker: bool,
    pub(crate) directive_checker: bool,
    pub(crate) feature_gate: bool,
    pub(crate) logger: bool,
    pub(crate) query_timeout: bool,
    pub(crate) metrics: bool,
    pub(crate) tracing: bool,
    pub(crate) apollo_tracing: bool,
    pub(crate) open_telemetry: bool,
}

#[GraphQLConfig]
#[derive(Default)]
pub struct TxExecFullNodeConfig {
    pub(crate) node_rpc_url: Option<String>,
}

#[GraphQLConfig]
#[derive(Default)]
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

    /// List the available versions for this GraphQL service.
    async fn available_versions(&self) -> Vec<String> {
        self.versions.versions.clone()
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
    pub async fn max_output_nodes(&self) -> u32 {
        self.limits.max_output_nodes
    }

    /// Maximum estimated cost of a database query used to serve a GraphQL request.  This is
    /// measured in the same units that the database uses in EXPLAIN queries.
    async fn max_db_query_cost(&self) -> u32 {
        self.limits.max_db_query_cost
    }

    /// Default number of elements allowed on a single page of a connection.
    async fn default_page_size(&self) -> u32 {
        self.limits.default_page_size
    }

    /// Maximum number of elements allowed on a single page of a connection.
    async fn max_page_size(&self) -> u32 {
        self.limits.max_page_size
    }

    /// Maximum time in milliseconds spent waiting for a response from fullnode after issuing a
    /// a transaction to execute. Note that the transaction may still succeed even in the case of a
    /// timeout. Transactions are idempotent, so a transaction that times out should be resubmitted
    /// until the network returns a definite response (success or failure, not timeout).
    async fn mutation_timeout_ms(&self) -> u32 {
        self.limits.mutation_timeout_ms
    }

    /// Maximum time in milliseconds that will be spent to serve one query request.
    async fn request_timeout_ms(&self) -> u32 {
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
            db_url: "postgres://postgres:postgrespw@localhost:5432/sui_graphql_rpc_e2e_tests"
                .to_string(),
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
        ide_title
            .map(|ide_title| Ide { ide_title })
            .unwrap_or_default()
    }
}

impl BackgroundTasksConfig {
    pub fn test_defaults() -> Self {
        Self {
            watermark_update_ms: 100, // Set to 100ms for testing
        }
    }
}

impl Default for Versions {
    fn default() -> Self {
        Self {
            versions: vec![format!(
                "{}.{}",
                env!("CARGO_PKG_VERSION_MAJOR"),
                env!("CARGO_PKG_VERSION_MINOR")
            )],
        }
    }
}

impl Default for Ide {
    fn default() -> Self {
        Self {
            ide_title: "Sui GraphQL IDE".to_string(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            port: 8000,
            host: "127.0.0.1".to_string(),
            db_url: "postgres://postgres:postgrespw@localhost:5432/sui_indexer".to_string(),
            db_pool_size: 10,
            prom_url: "0.0.0.0".to_string(),
            prom_port: 9184,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        // Picked so that TS SDK shim layer queries all pass limit.
        // TODO: calculate proper cost limits
        Self {
            max_query_depth: 20,
            max_query_nodes: 300,
            max_output_nodes: 100_000,
            max_query_payload_size: 5_000,
            max_db_query_cost: 20_000,
            default_page_size: 20,
            max_page_size: 50,
            // This default was picked as the sum of pre- and post- quorum timeouts from
            // [`sui_core::authority_aggregator::TimeoutConfig`], with a 10% buffer.
            //
            // <https://github.com/MystenLabs/sui/blob/eaf05fe5d293c06e3a2dfc22c87ba2aef419d8ea/crates/sui-core/src/authority_aggregator.rs#L84-L85>
            mutation_timeout_ms: 74_000,
            request_timeout_ms: 40_000,
            // The following limits reflect the max values set in ProtocolConfig, at time of writing.
            // <https://github.com/MystenLabs/sui/blob/333f87061f0656607b1928aba423fa14ca16899e/crates/sui-protocol-config/src/lib.rs#L1580>
            max_type_argument_depth: 16,
            // <https://github.com/MystenLabs/sui/blob/4b934f87acae862cecbcbefb3da34cabb79805aa/crates/sui-protocol-config/src/lib.rs#L1618>
            max_type_argument_width: 32,
            // <https://github.com/MystenLabs/sui/blob/4b934f87acae862cecbcbefb3da34cabb79805aa/crates/sui-protocol-config/src/lib.rs#L1622>
            max_type_nodes: 256,
            // <https://github.com/MystenLabs/sui/blob/4b934f87acae862cecbcbefb3da34cabb79805aa/crates/sui-protocol-config/src/lib.rs#L1988>
            max_move_value_depth: 128,
        }
    }
}

impl Default for InternalFeatureConfig {
    fn default() -> Self {
        Self {
            query_limits_checker: true,
            directive_checker: true,
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
            watermark_update_ms: 500,
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full)
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
                mutation-timeout-ms = 74000
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
                mutation_timeout_ms: 74_000,
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
                mutation-timeout-ms = 74000
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
                mutation_timeout_ms: 74_000,
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

    #[test]
    fn test_read_partial_in_service_config() {
        let actual = ServiceConfig::read(
            r#" disabled-features = ["analytics"]

                [limits]
                max-query-depth = 42
                max-query-nodes = 320
            "#,
        )
        .unwrap();

        // When reading partially, the other parts will come from the default implementation.
        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 42,
                max_query_nodes: 320,
                ..Default::default()
            },
            disabled_features: BTreeSet::from([FunctionalGroup::Analytics]),
            ..Default::default()
        };

        assert_eq!(actual, expect);
    }
}
