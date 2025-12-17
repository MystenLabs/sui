// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use sui_default_config::DefaultConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress};

pub use sui_name_service::NameServiceConfig;

pub const CLIENT_SDK_TYPE_HEADER: &str = "client-sdk-type";

#[derive(Debug)]
pub struct RpcConfig {
    /// Configuration for object-related RPC methods.
    pub objects: ObjectsConfig,

    /// Configuration for dynamic-field-related RPC methods.
    pub dynamic_fields: DynamicFieldsConfig,

    /// Configuration for transaction-related RPC methods.
    pub transactions: TransactionsConfig,

    /// Configuration for SuiNS related RPC methods.
    pub name_service: NameServiceConfig,

    /// Configuration for coin-related RPC methods.
    pub coins: CoinsConfig,

    /// Configuration for methods that require a fullnode RPC connection,
    /// including transaction execution, dry-running, and delegation coin queries etc.
    pub node: NodeConfig,

    /// Configuring limits for the package resolver.
    pub package_resolver: sui_package_resolver::Limits,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct RpcLayer {
    pub objects: ObjectsLayer,
    pub dynamic_fields: DynamicFieldsLayer,
    pub transactions: TransactionsLayer,
    pub name_service: NameServiceLayer,
    pub coins: CoinsLayer,
    pub node: NodeLayer,
    pub package_resolver: PackageResolverLayer,
}

#[derive(Debug, Clone)]
pub struct ObjectsConfig {
    /// The maximum number of keys that can be queried in a single multi-get request.
    pub max_multi_get_objects: usize,

    /// The default page size limit when querying objects, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying objects. Requesting a page larger than
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

    /// The number of times to retry a kv get operation. Retry is needed when a version of the object
    /// is not yet found in the kv store due to the kv being behind the pg table's checkpoint watermark.
    pub obj_retry_count: usize,

    /// The interval between kv retry attempts in milliseconds.
    pub obj_retry_interval_ms: u64,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct ObjectsLayer {
    pub max_multi_get_objects: Option<usize>,
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,
    pub max_display_field_depth: Option<usize>,
    pub max_display_output_size: Option<usize>,
    pub max_filter_depth: Option<usize>,
    pub max_type_filters: Option<usize>,
    pub filter_scan_size: Option<usize>,
    pub obj_retry_count: Option<usize>,
    pub obj_retry_interval_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DynamicFieldsConfig {
    /// The default page size limit when querying dynamic fields, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying dynamic fields. Requesting a page larger
    /// than this is a user error.
    pub max_page_size: usize,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct DynamicFieldsLayer {
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TransactionsConfig {
    /// The default page size limit when querying transactions, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying transactions. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,

    /// The number of times to retry a read from kv or pg transaction tables. Retry is needed when a tx digest
    /// is not yet found in the table due to it being behind other transaction table's checkpoint watermark.
    pub tx_retry_count: usize,

    /// The interval between tx_digest retry attempts in milliseconds.
    pub tx_retry_interval_ms: u64,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct TransactionsLayer {
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,
    pub tx_retry_count: Option<usize>,
    pub tx_retry_interval_ms: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct NameServiceLayer {
    pub package_address: Option<SuiAddress>,
    pub registry_id: Option<ObjectID>,
    pub reverse_registry_id: Option<ObjectID>,
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
#[serde(deny_unknown_fields)]
pub struct CoinsLayer {
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,
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
#[serde(deny_unknown_fields)]
pub struct NodeLayer {
    pub header_value: Option<String>,
    pub max_request_size: Option<u32>,
}

#[DefaultConfig]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PackageResolverLayer {
    pub max_type_argument_depth: usize,
    pub max_type_argument_width: usize,
    pub max_type_nodes: usize,
    pub max_move_value_depth: usize,
}

impl RpcLayer {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        Self {
            objects: ObjectsConfig::default().into(),
            dynamic_fields: DynamicFieldsConfig::default().into(),
            transactions: TransactionsConfig::default().into(),
            name_service: NameServiceConfig::default().into(),
            coins: CoinsConfig::default().into(),
            package_resolver: PackageResolverLayer::default(),
            node: NodeConfig::default().into(),
        }
    }

    pub fn finish(self) -> RpcConfig {
        RpcConfig {
            objects: self.objects.finish(ObjectsConfig::default()),
            dynamic_fields: self.dynamic_fields.finish(DynamicFieldsConfig::default()),
            transactions: self.transactions.finish(TransactionsConfig::default()),
            name_service: self.name_service.finish(NameServiceConfig::default()),
            coins: self.coins.finish(CoinsConfig::default()),
            node: self.node.finish(NodeConfig::default()),
            package_resolver: self.package_resolver.finish(),
        }
    }
}

impl ObjectsLayer {
    pub fn finish(self, base: ObjectsConfig) -> ObjectsConfig {
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
            obj_retry_count: self.obj_retry_count.unwrap_or(base.obj_retry_count),
            obj_retry_interval_ms: self
                .obj_retry_interval_ms
                .unwrap_or(base.obj_retry_interval_ms),
        }
    }
}

impl DynamicFieldsLayer {
    pub fn finish(self, base: DynamicFieldsConfig) -> DynamicFieldsConfig {
        DynamicFieldsConfig {
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
        }
    }
}

impl TransactionsLayer {
    pub fn finish(self, base: TransactionsConfig) -> TransactionsConfig {
        TransactionsConfig {
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
            tx_retry_count: self.tx_retry_count.unwrap_or(base.tx_retry_count),
            tx_retry_interval_ms: self
                .tx_retry_interval_ms
                .unwrap_or(base.tx_retry_interval_ms),
        }
    }
}

impl NameServiceLayer {
    pub fn finish(self, base: NameServiceConfig) -> NameServiceConfig {
        NameServiceConfig {
            package_address: self.package_address.unwrap_or(base.package_address),
            registry_id: self.registry_id.unwrap_or(base.registry_id),
            reverse_registry_id: self.reverse_registry_id.unwrap_or(base.reverse_registry_id),
        }
    }
}

impl CoinsLayer {
    pub fn finish(self, base: CoinsConfig) -> CoinsConfig {
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
            .set_headers(headers)
            .build(&fullnode_rpc_url)
            .context("Failed to initialize fullnode RPC client")
    }
}

impl NodeLayer {
    pub fn finish(self, base: NodeConfig) -> NodeConfig {
        NodeConfig {
            header_value: self.header_value.unwrap_or(base.header_value),
            max_request_size: self.max_request_size.unwrap_or(base.max_request_size),
        }
    }
}

impl PackageResolverLayer {
    pub fn finish(self) -> sui_package_resolver::Limits {
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
            dynamic_fields: DynamicFieldsConfig::default(),
            transactions: TransactionsConfig::default(),
            name_service: NameServiceConfig::default(),
            coins: CoinsConfig::default(),
            node: NodeConfig::default(),
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
            obj_retry_count: 5,
            obj_retry_interval_ms: 100,
        }
    }
}

impl Default for DynamicFieldsConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
        }
    }
}

impl Default for TransactionsConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
            tx_retry_count: 5,
            tx_retry_interval_ms: 100,
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
            obj_retry_count: Some(config.obj_retry_count),
            obj_retry_interval_ms: Some(config.obj_retry_interval_ms),
        }
    }
}

impl From<DynamicFieldsConfig> for DynamicFieldsLayer {
    fn from(config: DynamicFieldsConfig) -> Self {
        Self {
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
        }
    }
}

impl From<TransactionsConfig> for TransactionsLayer {
    fn from(config: TransactionsConfig) -> Self {
        Self {
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
            tx_retry_count: Some(config.tx_retry_count),
            tx_retry_interval_ms: Some(config.tx_retry_interval_ms),
        }
    }
}

impl From<NameServiceConfig> for NameServiceLayer {
    fn from(config: NameServiceConfig) -> Self {
        Self {
            package_address: Some(config.package_address),
            registry_id: Some(config.registry_id),
            reverse_registry_id: Some(config.reverse_registry_id),
        }
    }
}

impl From<CoinsConfig> for CoinsLayer {
    fn from(config: CoinsConfig) -> Self {
        Self {
            default_page_size: Some(config.default_page_size),
            max_page_size: Some(config.max_page_size),
        }
    }
}

impl From<NodeConfig> for NodeLayer {
    fn from(config: NodeConfig) -> Self {
        Self {
            header_value: Some(config.header_value),
            max_request_size: Some(config.max_request_size),
        }
    }
}
