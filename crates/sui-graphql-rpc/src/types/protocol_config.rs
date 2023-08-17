// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::server::data_provider::DataProvider;

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
    pub configs: Vec<ProtocolConfigAttr>,
    pub feature_flags: Vec<ProtocolConfigFeatureFlag>,
    pub protocol_version: u64,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl ProtocolConfigs {
    async fn configs(&self, ctx: &Context<'_>) -> Result<Option<Vec<ProtocolConfigAttr>>> {
        Ok(Some(
            ctx.data_unchecked::<Box<dyn DataProvider>>()
                .fetch_protocol_config(None)
                .await?
                .configs,
        ))
    }

    async fn feature_flags(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Vec<ProtocolConfigFeatureFlag>>> {
        Ok(Some(
            ctx.data_unchecked::<Box<dyn DataProvider>>()
                .fetch_protocol_config(None)
                .await?
                .feature_flags,
        ))
    }

    async fn protocol_version(&self, ctx: &Context<'_>) -> Result<u64> {
        Ok(ctx
            .data_unchecked::<Box<dyn DataProvider>>()
            .fetch_protocol_config(None)
            .await?
            .protocol_version)
    }

    async fn config(&self, ctx: &Context<'_>, key: String) -> Result<Option<ProtocolConfigAttr>> {
        match self
            .configs(ctx)
            .await?
            .map(|configs| configs.into_iter().find(|config| config.key == key))
        {
            Some(config) => Ok(config),
            None => Ok(None),
        }
    }

    async fn feature_flag(
        &self,
        ctx: &Context<'_>,
        key: String,
    ) -> Result<Option<ProtocolConfigFeatureFlag>> {
        match self
            .feature_flags(ctx)
            .await?
            .map(|flags| flags.into_iter().find(|config| config.key == key))
        {
            Some(config) => Ok(config),
            None => Ok(None),
        }
    }
}
