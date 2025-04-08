// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::str::FromStr;

use crate::functional_group::FunctionalGroup;
use async_graphql::*;
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Display, time::Duration};
use sui_default_config::DefaultConfig;
use sui_name_service::NameServiceConfig;
use sui_types::base_types::{ObjectID, SuiAddress};

pub(crate) const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(30_000);
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 1_000;

// Move Registry constants
pub(crate) const MOVE_REGISTRY_MODULE: &IdentStr = ident_str!("name");
pub(crate) const MOVE_REGISTRY_TYPE: &IdentStr = ident_str!("Name");
const MOVE_REGISTRY_PACKAGE: &str =
    "0x62c1f5b1cb9e3bfc3dd1f73c95066487b662048a6358eabdbf67f6cdeca6db4b";
const MOVE_REGISTRY_TABLE_ID: &str =
    "0xe8417c530cde59eddf6dfb760e8a0e3e2c6f17c69ddaab5a73dd6a6e65fc463b";
const DEFAULT_PAGE_LIMIT: u16 = 50;

/// The combination of all configurations for the GraphQL service.
#[DefaultConfig]
#[derive(Clone, Default, Debug)]
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
#[DefaultConfig]
#[derive(clap::Args, Clone, Eq, PartialEq, Debug)]
pub struct ConnectionConfig {
    /// Port to bind the server to
    #[clap(short, long, default_value_t = ConnectionConfig::default().port)]
    pub port: u16,
    /// Host to bind the server to
    #[clap(long, default_value_t = ConnectionConfig::default().host)]
    pub host: String,
    /// DB URL for data fetching
    #[clap(short, long, default_value_t = ConnectionConfig::default().db_url)]
    pub db_url: String,
    /// Pool size for DB connections
    #[clap(long, default_value_t = ConnectionConfig::default().db_pool_size)]
    pub db_pool_size: u32,
    /// Host to bind the prom server to
    #[clap(long, default_value_t = ConnectionConfig::default().prom_host)]
    pub prom_host: String,
    /// Port to bind the prom server to
    #[clap(long, default_value_t = ConnectionConfig::default().prom_port)]
    pub prom_port: u16,
    /// Skip checking whether the service is compatible with the DB it is about to connect to, on
    /// start-up.
    #[clap(long, default_value_t = ConnectionConfig::default().skip_migration_consistency_check)]
    pub skip_migration_consistency_check: bool,
}

/// Configuration on features supported by the GraphQL service, passed in a TOML-based file. These
/// configurations are shared across fleets of the service, i.e. all testnet services will have the
/// same `ServiceConfig`.
#[DefaultConfig]
#[derive(Clone, Default, Eq, PartialEq, Debug)]
pub struct ServiceConfig {
    pub limits: Limits,
    pub disabled_features: BTreeSet<FunctionalGroup>,
    pub experiments: Experiments,
    pub name_service: NameServiceConfig,
    pub background_tasks: BackgroundTasksConfig,
    pub zklogin: ZkLoginConfig,
    pub move_registry: MoveRegistryConfig,
}

#[DefaultConfig]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Limits {
    /// Maximum depth of nodes in the requests.
    pub max_query_depth: u32,
    /// Maximum number of nodes in the requests.
    pub max_query_nodes: u32,
    /// Maximum number of output nodes allowed in the response.
    pub max_output_nodes: u32,
    /// Maximum size in bytes allowed for the `txBytes` and `signatures` fields of a GraphQL
    /// mutation request in the `executeTransactionBlock` node, and for the `txBytes` of a
    /// `dryRunTransactionBlock` node.
    pub max_tx_payload_size: u32,
    /// Maximum size in bytes of the JSON payload of a GraphQL read request (excluding
    /// `max_tx_payload_size`).
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
    /// Maximum number of transaction ids that can be passed to a `TransactionBlockFilter`.
    pub max_transaction_ids: u32,
    /// Maximum number of keys that can be passed to a `multiGetObjects` query.
    pub max_multi_get_objects_keys: u32,
    /// Maximum number of candidates to scan when gathering a page of results.
    pub max_scan_limit: u32,
}

#[DefaultConfig]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BackgroundTasksConfig {
    /// How often the watermark task checks the indexer database to update the checkpoint and epoch
    /// watermarks.
    pub watermark_update_ms: u64,
}

#[DefaultConfig]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct MoveRegistryConfig {
    pub(crate) external_api_url: Option<String>,
    pub(crate) resolution_type: ResolutionType,
    pub(crate) page_limit: u16,
    pub(crate) package_address: SuiAddress,
    pub(crate) registry_id: ObjectID,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum ResolutionType {
    Internal,
    External,
}

/// The Version of the service. `year.month` represents the major release.
/// New `patch` versions represent backwards compatible fixes for their major release.
/// The `full` version is `year.month.patch-sha`.
#[derive(Copy, Clone, Debug)]
pub struct Version {
    /// The major version for the release
    pub major: &'static str,
    /// The minor version of the release
    pub minor: &'static str,
    /// The patch version of the release
    pub patch: &'static str,
    /// The full commit SHA that the release was built from
    pub sha: &'static str,
    /// The full version string: {MAJOR}.{MINOR}.{PATCH}-{SHA}
    ///
    /// The full version is pre-computed as a &'static str because that is what is required for
    /// `uptime_metric`.
    pub full: &'static str,
}

impl Version {
    /// Use for testing when you need the Version obj and a year.month &str
    pub fn for_testing() -> Self {
        Self {
            major: "42",
            minor: "43",
            patch: "44",
            sha: "testing-no-sha",
            // note that this full field is needed for metrics but not for testing
            full: "42.43.44-testing-no-sha",
        }
    }
}

#[DefaultConfig]
#[derive(clap::Args, Clone, Debug)]
pub struct Ide {
    /// The title to display at the top of the web-based GraphiQL IDE.
    #[clap(short, long, default_value_t = Ide::default().ide_title)]
    pub ide_title: String,
}

#[DefaultConfig]
#[derive(Clone, Default, Eq, PartialEq, Debug)]
pub struct Experiments {
    // Add experimental flags here, to provide access to them through-out the GraphQL
    // implementation.
    #[cfg(test)]
    test_flag: bool,
}

#[DefaultConfig]
#[derive(Clone, Debug)]
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

#[DefaultConfig]
#[derive(clap::Args, Clone, Default, Debug)]
pub struct TxExecFullNodeConfig {
    /// RPC URL for the fullnode to send transactions to execute and dry-run.
    #[clap(long)]
    pub(crate) node_rpc_url: Option<String>,
}

#[DefaultConfig]
#[derive(Clone, Default, Eq, PartialEq, Debug)]
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

    /// The maximum bytes allowed for the `txBytes` and `signatures` fields of the GraphQL mutation
    /// `executeTransactionBlock` node, or for the `txBytes` of a `dryRunTransactionBlock`.
    ///
    /// It is the value of the maximum transaction bytes (including the signatures) allowed by the
    /// protocol, plus the Base64 overhead (roughly 1/3 of the original string).
    async fn max_transaction_payload_size(&self) -> u32 {
        self.limits.max_tx_payload_size
    }

    /// The maximum bytes allowed for the JSON object in the request body of a GraphQL query, for
    /// the read part of the query.
    /// In case of mutations or dryRunTransactionBlocks the txBytes and signatures are not
    /// included in this limit.
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

    /// Maximum number of transaction ids that can be passed to a `TransactionBlockFilter`.
    async fn max_transaction_ids(&self) -> u32 {
        self.limits.max_transaction_ids
    }

    /// Maximum number of keys that can be passed to a `multiGetObjects` query.
    async fn max_multi_get_objects_keys(&self) -> u32 {
        self.limits.max_multi_get_objects_keys
    }

    /// Maximum number of candidates to scan when gathering a page of results.
    async fn max_scan_limit(&self) -> u32 {
        self.limits.max_scan_limit
    }
}

impl TxExecFullNodeConfig {
    pub fn new(node_rpc_url: Option<String>) -> Self {
        Self { node_rpc_url }
    }
}

impl ConnectionConfig {
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

    pub fn host(&self) -> String {
        self.host.clone()
    }

    pub fn port(&self) -> u16 {
        self.port
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

    pub fn move_registry_test_defaults(
        external: bool,
        endpoint: Option<String>,
        pkg_address: Option<SuiAddress>,
        object_id: Option<ObjectID>,
        page_limit: Option<u16>,
    ) -> Self {
        Self {
            move_registry: MoveRegistryConfig {
                resolution_type: if external {
                    ResolutionType::External
                } else {
                    ResolutionType::Internal
                },
                external_api_url: endpoint,
                package_address: pkg_address.unwrap_or_default(),
                registry_id: object_id.unwrap_or(ObjectID::random()),
                page_limit: page_limit.unwrap_or(50),
            },
            ..Self::test_defaults()
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

impl BackgroundTasksConfig {
    pub fn test_defaults() -> Self {
        Self {
            watermark_update_ms: 100, // Set to 100ms for testing
        }
    }
}

impl MoveRegistryConfig {
    pub(crate) fn new(
        resolution_type: ResolutionType,
        external_api_url: Option<String>,
        page_limit: u16,
        package_address: SuiAddress,
        registry_id: ObjectID,
    ) -> Self {
        Self {
            resolution_type,
            external_api_url,
            page_limit,
            package_address,
            registry_id,
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
            prom_host: "0.0.0.0".to_string(),
            prom_port: 9184,
            skip_migration_consistency_check: false,
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
            // Filter-specific limits, such as the number of transaction ids that can be specified
            // for the `TransactionBlockFilter`.
            max_transaction_ids: 1000,
            max_multi_get_objects_keys: 500,
            max_scan_limit: 100_000_000,
            // This value is set to be the size of the max transaction bytes allowed + base64
            // overhead (roughly 1/3 of the original string). This is rounded up.
            //
            // <https://github.com/MystenLabs/sui/blob/4b934f87acae862cecbcbefb3da34cabb79805aa/crates/sui-protocol-config/src/lib.rs#L1578>
            max_tx_payload_size: (128u32 * 1024u32 * 4u32).div_ceil(3),
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

// TODO: Keeping the values as is, because we'll remove the default getters
// when we refactor to use `[GraphqlConfig]` macro.
impl Default for MoveRegistryConfig {
    fn default() -> Self {
        Self::new(
            ResolutionType::Internal,
            None,
            DEFAULT_PAGE_LIMIT,
            SuiAddress::from_str(MOVE_REGISTRY_PACKAGE).unwrap(),
            ObjectID::from_str(MOVE_REGISTRY_TABLE_ID).unwrap(),
        )
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
                max-mutation-payload-size = 174763
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
                max-transaction-ids = 11
                max-multi-get-objects-keys = 11
                max-scan-limit = 50
            "#,
        )
        .unwrap();

        let expect = ServiceConfig {
            limits: Limits {
                max_query_depth: 100,
                max_query_nodes: 300,
                max_output_nodes: 200000,
                max_tx_payload_size: 174763,
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
                max_transaction_ids: 11,
                max_multi_get_objects_keys: 11,
                max_scan_limit: 50,
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
                max-tx-payload-size = 181017
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
                max-transaction-ids = 42
                max-multi-get-objects-keys = 42
                max-scan-limit = 420

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
                max_tx_payload_size: 181017,
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
                max_transaction_ids: 42,
                max_multi_get_objects_keys: 42,
                max_scan_limit: 420,
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
