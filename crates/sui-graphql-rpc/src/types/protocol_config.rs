// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

/// A single protocol configuration value.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct ProtocolConfigAttr {
    pub key: String,
    pub value: String,
}

/// Whether or not a single feature is enabled in the protocol config.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct ProtocolConfigFeatureFlag {
    pub key: String,
    pub value: bool,
}

/// Constants that control how the chain operates.
///
/// These can only change during protocol upgrades which happen on epoch boundaries.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct ProtocolConfigs {
    /// The protocol is not required to change on every epoch boundary, so the protocol version
    /// tracks which change to the protocol these configs are from.
    pub protocol_version: u64,

    /// List all available feature flags and their values.  Feature flags are a form of boolean
    /// configuration that are usually used to gate features while they are in development.  Once a
    /// flag has been enabled, it is rare for it to be disabled.
    pub feature_flags: Vec<ProtocolConfigFeatureFlag>,

    /// List all available configurations and their values.  These configurations can take any value
    /// (but they will all be represented in string form), and do not include feature flags.
    pub configs: Vec<ProtocolConfigAttr>,
}

#[ComplexObject]
impl ProtocolConfigs {
    /// Query for the value of the configuration with name `key`.
    async fn config(&self, key: String) -> Option<&ProtocolConfigAttr> {
        self.configs.iter().find(|config| config.key == key)
    }

    /// Query for the state of the feature flag with name `key`.
    async fn feature_flag(&self, key: String) -> Option<&ProtocolConfigFeatureFlag> {
        self.feature_flags.iter().find(|config| config.key == key)
    }
}
