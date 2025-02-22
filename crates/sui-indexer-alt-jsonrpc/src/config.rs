// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

use sui_default_config::DefaultConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use tracing::warn;

pub use sui_name_service::NameServiceConfig;

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
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ObjectsLayer {
    pub max_multi_get_objects: Option<usize>,
    pub default_page_size: Option<usize>,
    pub max_page_size: Option<usize>,

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
