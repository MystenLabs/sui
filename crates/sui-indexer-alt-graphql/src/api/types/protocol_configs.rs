// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::Context as _;
use async_graphql::{Context, Object, SimpleObject};
use diesel::{ExpressionMethods, QueryDsl as _};
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::{
    epochs::{StoredFeatureFlag, StoredProtocolConfig},
    schema::{kv_feature_flags, kv_protocol_configs},
};

use crate::{api::scalars::uint53::UInt53, error::RpcError};

pub(crate) struct ProtocolConfigs {
    protocol_version: u64,
}

/// A protocol configuration that can hold an arbitrary value (or no value at all).
#[derive(Clone, SimpleObject)]
pub(crate) struct ProtocolConfig {
    /// Configuration name.
    pub key: String,

    /// Configuration value.
    pub value: Option<String>,
}

/// A boolean protocol configuration.
#[derive(Clone, SimpleObject)]
pub(crate) struct FeatureFlag {
    /// Feature flag name.
    pub key: String,

    /// Feature flag value.
    pub value: bool,
}

#[derive(Clone)]
struct ConfigContent(BTreeMap<String, Option<String>>);

#[derive(Clone)]
struct FlagContent(BTreeMap<String, bool>);

/// Constants that control how the chain operates.
///
/// These can only change during protocol upgrades which happen on epoch boundaries. Configuration is split into feature flags (which are just booleans), and configs which can take any value (including no value at all), and will be represented by a string.
#[Object]
impl ProtocolConfigs {
    async fn protocol_version(&self) -> UInt53 {
        self.protocol_version.into()
    }

    #[graphql(flatten)]
    async fn configs(&self, ctx: &Context<'_>) -> Result<ConfigContent, RpcError> {
        ConfigContent::fetch(ctx, self.protocol_version).await
    }

    #[graphql(flatten)]
    async fn feature_flags(&self, ctx: &Context<'_>) -> Result<FlagContent, RpcError> {
        FlagContent::fetch(ctx, self.protocol_version).await
    }
}

#[Object]
impl ConfigContent {
    /// Query for the value of the configuration with name `key`.
    async fn config(&self, key: String) -> Option<ProtocolConfig> {
        self.0.get(&key).map(|value| ProtocolConfig {
            key: key.clone(),
            value: value.clone(),
        })
    }

    /// List all available configurations and their values.
    async fn configs(&self) -> Vec<ProtocolConfig> {
        self.0
            .clone()
            .into_iter()
            .map(|(key, value)| ProtocolConfig { key, value })
            .collect()
    }
}

#[Object]
impl FlagContent {
    /// Query for the state of the feature flag with name `key`.
    async fn feature_flag(&self, key: String) -> Option<FeatureFlag> {
        self.0.get(&key).map(|value| FeatureFlag {
            key: key.clone(),
            value: *value,
        })
    }

    /// List all available feature flags and their values.
    async fn feature_flags(&self) -> Vec<FeatureFlag> {
        self.0
            .clone()
            .into_iter()
            .map(|(key, value)| FeatureFlag { key, value })
            .collect()
    }
}

impl ProtocolConfigs {
    /// Construct a protocol config object that is represented by just its identifier (its protocol version).
    pub(crate) fn with_protocol_version(protocol_version: u64) -> Self {
        Self { protocol_version }
    }
}

impl ConfigContent {
    async fn fetch(ctx: &Context<'_>, protocol_version: u64) -> Result<Self, RpcError> {
        use kv_protocol_configs::dsl as p;

        let pg_reader: &PgReader = ctx.data()?;
        let mut conn = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let configs: Vec<StoredProtocolConfig> = conn
            .results(p::kv_protocol_configs.filter(p::protocol_version.eq(protocol_version as i64)))
            .await
            .context("Fail to fetch protocol configs")?;

        Ok(Self(
            configs
                .into_iter()
                .map(|c| (c.config_name, c.config_value))
                .collect(),
        ))
    }
}

impl FlagContent {
    async fn fetch(ctx: &Context<'_>, protocol_version: u64) -> Result<Self, RpcError> {
        use kv_feature_flags::dsl as p;

        let pg_reader: &PgReader = ctx.data()?;
        let mut conn = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let configs: Vec<StoredFeatureFlag> = conn
            .results(p::kv_feature_flags.filter(p::protocol_version.eq(protocol_version as i64)))
            .await
            .context("Fail to fetch feature flags")?;

        Ok(Self(
            configs
                .into_iter()
                .map(|c| (c.flag_name, c.flag_value))
                .collect(),
        ))
    }
}
