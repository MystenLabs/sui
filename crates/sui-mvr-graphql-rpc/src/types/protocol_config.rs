// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::*;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use sui_indexer::schema::{epochs, feature_flags, protocol_configs};

use crate::{
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
};

use super::uint53::UInt53;

/// A single protocol configuration value.
#[derive(Clone, Debug, SimpleObject)]
pub(crate) struct ProtocolConfigAttr {
    pub key: String,
    pub value: Option<String>,
}

/// Whether or not a single feature is enabled in the protocol config.
#[derive(Clone, Debug, SimpleObject)]
pub(crate) struct ProtocolConfigFeatureFlag {
    pub key: String,
    pub value: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ProtocolConfigs {
    version: u64,
    configs: BTreeMap<String, Option<String>>,
    feature_flags: BTreeMap<String, bool>,
}

/// Constants that control how the chain operates.
///
/// These can only change during protocol upgrades which happen on epoch boundaries.
#[Object]
impl ProtocolConfigs {
    /// The protocol is not required to change on every epoch boundary, so the protocol version
    /// tracks which change to the protocol these configs are from.
    async fn protocol_version(&self) -> UInt53 {
        self.version.into()
    }

    /// List all available feature flags and their values.  Feature flags are a form of boolean
    /// configuration that are usually used to gate features while they are in development.  Once a
    /// flag has been enabled, it is rare for it to be disabled.
    async fn feature_flags(&self) -> Vec<ProtocolConfigFeatureFlag> {
        self.feature_flags
            .clone()
            .into_iter()
            .map(|(key, value)| ProtocolConfigFeatureFlag { key, value })
            .collect()
    }

    /// List all available configurations and their values.  These configurations can take any value
    /// (but they will all be represented in string form), and do not include feature flags.
    async fn configs(&self) -> Vec<ProtocolConfigAttr> {
        self.configs
            .clone()
            .into_iter()
            .map(|(key, value)| ProtocolConfigAttr { key, value })
            .collect()
    }

    /// Query for the value of the configuration with name `key`.
    async fn config(&self, key: String) -> Option<ProtocolConfigAttr> {
        self.configs.get(&key).map(|value| ProtocolConfigAttr {
            key,
            value: value.as_ref().map(|v| v.to_string()),
        })
    }

    /// Query for the state of the feature flag with name `key`.
    async fn feature_flag(&self, key: String) -> Option<ProtocolConfigFeatureFlag> {
        self.feature_flags
            .get(&key)
            .map(|value| ProtocolConfigFeatureFlag { key, value: *value })
    }
}

impl ProtocolConfigs {
    pub(crate) async fn query(db: &Db, protocol_version: Option<u64>) -> Result<Self, Error> {
        use epochs::dsl as e;
        use feature_flags::dsl as f;
        use protocol_configs::dsl as p;

        let version = if let Some(version) = protocol_version {
            version
        } else {
            let latest_version: i64 = db
                .execute(move |conn| {
                    async move {
                        conn.first(move || {
                            e::epochs
                                .select(e::protocol_version)
                                .order_by(e::epoch.desc())
                        })
                        .await
                    }
                    .scope_boxed()
                })
                .await
                .map_err(|e| {
                    Error::Internal(format!(
                        "Failed to fetch latest protocol version in db: {e}"
                    ))
                })?;
            latest_version as u64
        };

        // TODO: This could be optimized by fetching all configs and flags in a single query.
        let configs: BTreeMap<String, Option<String>> = db
            .execute(move |conn| {
                async move {
                    conn.results(move || {
                        p::protocol_configs
                            .select((p::config_name, p::config_value))
                            .filter(p::protocol_version.eq(version as i64))
                    })
                    .await
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch protocol configs in db: {e}")))?
            .into_iter()
            .collect();

        let feature_flags: BTreeMap<String, bool> = db
            .execute(move |conn| {
                async move {
                    conn.results(move || {
                        f::feature_flags
                            .select((f::flag_name, f::flag_value))
                            .filter(f::protocol_version.eq(version as i64))
                    })
                    .await
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch feature flags in db: {e}")))?
            .into_iter()
            .collect();

        Ok(ProtocolConfigs {
            version,
            configs,
            feature_flags,
        })
    }
}
