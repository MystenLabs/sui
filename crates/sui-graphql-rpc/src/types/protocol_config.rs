// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct ProtocolConfigAttr {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct ProtocolConfigFeatureFlag {
    pub key: String,
    pub value: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct ProtocolConfigs {
    pub protocol_version: u64,
    pub feature_flags: Vec<ProtocolConfigFeatureFlag>,
    pub configs: Vec<ProtocolConfigAttr>,
}

#[ComplexObject]
impl ProtocolConfigs {
    async fn config(&self, key: String) -> Result<Option<ProtocolConfigAttr>> {
        match self.configs.iter().find(|config| config.key == key) {
            Some(config) => Ok(Some(config.clone())),
            None => Ok(None),
        }
    }

    async fn feature_flag(&self, key: String) -> Result<Option<ProtocolConfigFeatureFlag>> {
        match self.feature_flags.iter().find(|config| config.key == key) {
            Some(config) => Ok(Some(config.clone())),
            None => Ok(None),
        }
    }
}
