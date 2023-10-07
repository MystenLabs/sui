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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProtocolConfigs {
    pub protocol_version: u64,
    pub feature_flags: Vec<ProtocolConfigFeatureFlag>,
    pub configs: Vec<ProtocolConfigAttr>,
}

#[Object]
impl ProtocolConfigs {
    async fn protocol_version(&self) -> Result<u64> {
        // TODO: implement DB counterpart without using Sui SDK client
        Ok(self.protocol_version)
    }

    async fn feature_flags(&self) -> Result<Option<Vec<ProtocolConfigFeatureFlag>>> {
        Ok(Some(self.feature_flags.clone()))
    }

    async fn configs(&self) -> Result<Option<Vec<ProtocolConfigAttr>>> {
        Ok(Some(self.configs.clone()))
    }

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
