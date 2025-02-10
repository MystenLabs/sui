// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

use sui_default_config::DefaultConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use tracing::warn;

use crate::api::{objects::ObjectsConfig, transactions::TransactionsConfig};

pub use sui_name_service::NameServiceConfig;

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct RpcConfig {
    /// Configuration for object-related RPC methods.
    pub objects: ObjectsLayer,

    /// Configuration for transaction-related RPC methods.
    pub transactions: TransactionsLayer,

    /// Configuration for SuiNS related RPC methods.
    pub name_service: NameServiceLayer,

    #[serde(flatten)]
    pub extra: toml::Table,
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

impl RpcConfig {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        Self {
            objects: ObjectsConfig::default().into(),
            transactions: TransactionsConfig::default().into(),
            name_service: NameServiceConfig::default().into(),
            extra: Default::default(),
        }
    }

    pub fn finish(mut self) -> RpcConfig {
        check_extra("top-level", mem::take(&mut self.extra));
        self
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
