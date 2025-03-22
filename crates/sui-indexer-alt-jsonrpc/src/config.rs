// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

use anyhow::Context as _;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use sui_default_config::DefaultConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use tracing::warn;

pub use sui_name_service::NameServiceConfig;

pub const CLIENT_SDK_TYPE_HEADER: &str = "client-sdk-type";

#[derive(Debug)]
pub struct RpcConfig {
    /// Configuration for object-related RPC methods.
    pub objects: ObjectsConfig,

    /// Configuration for transaction-related RPC methods.
    pub transactions: TransactionsConfig,

    /// Configuration for SuiNS related RPC methods.
    pub name_service: NameServiceConfig,

    /// Configuration for coin-related RPC methods.
    pub coins: CoinsConfig,

    /// Configuration for methods that require a fullnode RPC connection,
    /// including transaction execution, dry-running, and delegation coin queries etc.
    pub node: NodeConfig,

    /// Configuration for bigtable kv store, if it is used.
    pub bigtable: Option<BigtableConfig>,

    /// Configuring limits for the package resolver.
    pub package_resolver: sui_package_resolver::Limits,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct RpcLayer {
    /// Configuration for object-related RPC methods.
    pub objects: ObjectsLayer,

    /// Configuration for transaction-related RPC methods.
    pub transactions: TransactionsLayer,

    /// Configuration for SuiNS related RPC methods.
    pub name_service: NameServiceLayer,

    /// Configuration for coin-related RPC methods.
    pub coins: CoinsLayer,

    /// Configuration for transaction execution, dry-running, and delegation coin queries etc.
    pub node: NodeLayer,

    /// Configuration for bigtable kv store, if it is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable: Option<BigtableConfig>,

    /// Configuring limits for the package resolver.
    pub package_resolver: PackageResolverLayer,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Debug, Clone)]
pub struct ObjectsConfig {
    /// The maximum number of keys that can be queried in a single multi-get request.
    pub max_multi_get_objects: usize,

    /// The default page size limit when querying objects, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying transactions. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,

    /// The maximum depth a Display format string is allowed to nest field accesses.
    pub max_display_field_depth: usize,

    /// The maximum number of bytes occupied by Display field names and values in the output.
    pub max_display_output_size: usize,

    /// The maximum nesting depth of an owned object filter.
    pub max_filter_depth: usize,

    /// The maximum number of type filters in an owned object filter.
    pub max_type_filters: usize,

    /// The number of owned objects to fetch in one go when fulfilling a compound owned object
    /// filter.
    pub filter_scan_size: usize,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ObjectsLayer {
    pub max_multi_get_objects: Option<usize>,
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,
    pub max_display_field_depth: Option<usize>,
    pub max_display_output_size: Option<usize>,
    pub max_filter_depth: Option<usize>,
    pub max_type_filters: Option<usize>,
    pub filter_scan_size: Option<usize>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Debug, Clone)]
pub struct TransactionsConfig {
    /// The default page size limit when querying transactions, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying transactions. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct TransactionsLayer {
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct NameServiceLayer {
    pub package_address: Option<SuiAddress>,
    pub registry_id: Option<ObjectID>,
    pub reverse_registry_id: Option<ObjectID>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Debug, Clone)]
pub struct CoinsConfig {
    /// The default page size limit when querying coins, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying coins. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct CoinsLayer {
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Clone, Debug)]
pub struct NodeConfig {
    /// The value of the header to be sent to the fullnode RPC, used to distinguish between different instances.
    pub header_value: String,
    /// The maximum size of the request body allowed.
    pub max_request_size: u32,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct NodeLayer {
    pub header_value: Option<String>,
    pub max_request_size: Option<u32>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct BigtableConfig {
    /// The instance id of the Bigtable instance to connect to.
    pub instance_id: String,
}

#[DefaultConfig]
#[derive(Clone, Debug)]
pub struct PackageResolverLayer {
    pub max_type_argument_depth: usize,
    pub max_type_argument_width: usize,
    pub max_type_nodes: usize,
    pub max_move_value_depth: usize,

    #[serde(flatten)]
    pub extra: toml::Table,
}

impl RpcLayer {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        Self {
            objects: ObjectsConfig::default().into(),
            transactions: TransactionsConfig::default().into(),
            name_service: NameServiceConfig::default().into(),
            coins: CoinsConfig::default().into(),
            bigtable: None,
            package_resolver: PackageResolverLayer::default(),
            node: NodeConfig::default().into(),
            extra: Default::default(),
        }
    }

    pub fn finish(mut self) -> RpcConfig {
        check_extra("top-level", mem::take(&mut self.extra));
        RpcConfig {
            objects: self.objects.finish(ObjectsConfig::default()),
            transactions: self.transactions.finish(TransactionsConfig::default()),
            name_service: self.name_service.finish(NameServiceConfig::default()),
            coins: self.coins.finish(CoinsConfig::default()),
            node: self.node.finish(NodeConfig::default()),
            bigtable: self.bigtable,
            package_resolver: self.package_resolver.finish(),
        }
    }
}

impl ObjectsLayer {
    pub fn finish(self, base: ObjectsConfig) -> ObjectsConfig {
        check_extra("objects", self.extra);
        ObjectsConfig {
            max_multi_get_objects: self
                .max_multi_get_objects
                .unwrap_or(base.max_multi_get_objects),
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
            max_display_field_depth: self
                .max_display_field_depth
                .unwrap_or(base.max_display_field_depth),
            max_display_output_size: self
                .max_display_output_size
                .unwrap_or(base.max_display_output_size),
            max_filter_depth: self.max_filter_depth.unwrap_or(base.max_filter_depth),
            max_type_filters: self.max_type_filters.unwrap_or(base.max_type_filters),
            filter_scan_size: self.filter_scan_size.unwrap_or(base.filter_scan_size),
        }
    }
}

impl TransactionsLayer {
    pub fn finish(self, base: TransactionsConfig) -> TransactionsConfig {
        check_extra("transactions", self.extra);
        TransactionsConfig {
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
        }
    }
}

impl NameServiceLayer {
    pub fn finish(self, base: NameServiceConfig) -> NameServiceConfig {
        check_extra("name service", self.extra);
        NameServiceConfig {
            package_address: self.package_address.unwrap_or(base.package_address),
            registry_id: self.registry_id.unwrap_or(base.registry_id),
            reverse_registry_id: self.reverse_registry_id.unwrap_or(base.reverse_registry_id),
        }
    }
}

impl CoinsLayer {
    pub fn finish(self, base: CoinsConfig) -> CoinsConfig {
        check_extra("coins", self.extra);
        CoinsConfig {
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
        }
    }
}

impl NodeConfig {
    pub fn client(&self, fullnode_rpc_url: url::Url) -> anyhow::Result<HttpClient> {
        let mut headers = HeaderMap::new();
        headers.insert(
            CLIENT_SDK_TYPE_HEADER,
            HeaderValue::from_str(&self.header_value)?,
        );

        HttpClientBuilder::default()
            .max_request_size(self.max_request_size)
            .set_headers(headers.clone())
            .build(&fullnode_rpc_url)
            .context("Failed to initialize fullnode RPC client")
    }
}

impl NodeLayer {
    pub fn finish(self, base: NodeConfig) -> NodeConfig {
        check_extra("node", self.extra);
        NodeConfig {
            header_value: self.header_value.unwrap_or(base.header_value),
            max_request_size: self.max_request_size.unwrap_or(base.max_request_size),
        }
    }
}

impl PackageResolverLayer {
    pub fn finish(self) -> sui_package_resolver::Limits {
        check_extra("package-resolver", self.extra);
        sui_package_resolver::Limits {
            max_type_argument_depth: self.max_type_argument_depth,
            max_type_argument_width: self.max_type_argument_width,
            max_type_nodes: self.max_type_nodes,
            max_move_value_depth: self.max_move_value_depth,
        }
    }
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            objects: ObjectsConfig::default(),
            transactions: TransactionsConfig::default(),
            name_service: NameServiceConfig::default(),
            coins: CoinsConfig::default(),
            node: NodeConfig::default(),
            bigtable: None,
            package_resolver: PackageResolverLayer::default().finish(),
        }
    }
}

impl Default for ObjectsConfig {
    fn default() -> Self {
        Self {
            max_multi_get_objects: 50,
            default_page_size: 50,
            max_page_size: 100,
            max_display_field_depth: 10,
            max_display_output_size: 1024 * 1024,
            max_filter_depth: 3,
            max_type_filters: 10,
            filter_scan_size: 200,
        }
    }
}

impl Default for TransactionsConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
        }
    }
}

impl Default for CoinsConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            header_value: "sui-indexer-alt-jsonrpc".to_string(),
            max_request_size: (10 * 2) << 20, // 10MB
        }
    }
}

impl Default for PackageResolverLayer {
    fn default() -> Self {
        // SAFETY: Accessing the max supported config by the binary (and disregarding specific
        // chain state) is a safe operation for the RPC because we are only using this to set
        // default values which can be overridden by configuration.
        let config = ProtocolConfig::get_for_max_version_UNSAFE();

        Self {
            max_type_argument_depth: config.max_type_argument_depth() as usize,
            max_type_argument_width: config.max_generic_instantiation_length() as usize,
            max_type_nodes: config.max_type_nodes() as usize,
            max_move_value_depth: config.max_move_value_depth() as usize,

            extra: Default::default(),
        }
    }
}

impl From<ObjectsConfig> for ObjectsLayer {
    fn from(config: ObjectsConfig) -> Self {
        Self {
            max_multi_get_objects: Some(config.max_multi_get_objects),
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
            max_display_field_depth: Some(config.max_display_field_depth),
            max_display_output_size: Some(config.max_display_output_size),
            max_filter_depth: Some(config.max_filter_depth),
            max_type_filters: Some(config.max_type_filters),
            filter_scan_size: Some(config.filter_scan_size),
            extra: Default::default(),
        }
    }
}

impl From<TransactionsConfig> for TransactionsLayer {
    fn from(config: TransactionsConfig) -> Self {
        Self {
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
            extra: Default::default(),
        }
    }
}

impl From<NameServiceConfig> for NameServiceLayer {
    fn from(config: NameServiceConfig) -> Self {
        Self {
            package_address: Some(config.package_address),
            registry_id: Some(config.registry_id),
            reverse_registry_id: Some(config.reverse_registry_id),
            extra: Default::default(),
        }
    }
}

impl From<CoinsConfig> for CoinsLayer {
    fn from(config: CoinsConfig) -> Self {
        Self {
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
            extra: Default::default(),
        }
    }
}

impl From<NodeConfig> for NodeLayer {
    fn from(config: NodeConfig) -> Self {
        Self {
            header_value: Some(config.header_value),
            max_request_size: Some(config.max_request_size),
            extra: Default::default(),
        }
    }
}

/// Check whether there are any unrecognized extra fields and if so, warn about them.
fn check_extra(pos: &str, extra: toml::Table) {
    if !extra.is_empty() {
        warn!(
            "Found unrecognized {pos} field{} which will be ignored. This could be \
             because of a typo, or because it was introduced in a newer version of the indexer:\n{}",
            if extra.len() != 1 { "s" } else { "" },
            extra,
        )
    }
}
