// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer::schema::{checkpoints, epochs};
use sui_protocol_config::{ProtocolConfig as NativeProtocolConfig, ProtocolVersion};

use crate::{
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
    types::chain_identifier::ChainIdentifier,
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
    native: NativeProtocolConfig,
}

/// Constants that control how the chain operates.
///
/// These can only change during protocol upgrades which happen on epoch boundaries.
#[Object]
impl ProtocolConfigs {
    /// The protocol is not required to change on every epoch boundary, so the protocol version
    /// tracks which change to the protocol these configs are from.
    async fn protocol_version(&self) -> UInt53 {
        self.native.version.as_u64().into()
    }

    /// List all available feature flags and their values.  Feature flags are a form of boolean
    /// configuration that are usually used to gate features while they are in development.  Once a
    /// flag has been enabled, it is rare for it to be disabled.
    async fn feature_flags(&self) -> Vec<ProtocolConfigFeatureFlag> {
        self.native
            .feature_map()
            .into_iter()
            .map(|(key, value)| ProtocolConfigFeatureFlag { key, value })
            .collect()
    }

    /// List all available configurations and their values.  These configurations can take any value
    /// (but they will all be represented in string form), and do not include feature flags.
    async fn configs(&self) -> Vec<ProtocolConfigAttr> {
        self.native
            .attr_map()
            .into_iter()
            .map(|(key, value)| ProtocolConfigAttr {
                key,
                value: value.map(|v| v.to_string()),
            })
            .collect()
    }

    /// Query for the value of the configuration with name `key`.
    async fn config(&self, key: String) -> Option<ProtocolConfigAttr> {
        self.native
            .attr_map()
            .get(&key)
            .map(|value| ProtocolConfigAttr {
                key,
                value: value.as_ref().map(|v| v.to_string()),
            })
    }

    /// Query for the state of the feature flag with name `key`.
    async fn feature_flag(&self, key: String) -> Option<ProtocolConfigFeatureFlag> {
        self.native
            .feature_map()
            .get(&key)
            .map(|value| ProtocolConfigFeatureFlag { key, value: *value })
    }
}

impl ProtocolConfigs {
    pub(crate) async fn query(db: &Db, protocol_version: Option<u64>) -> Result<Self, Error> {
        use checkpoints::dsl as c;
        use epochs::dsl as e;

        let (latest_version, digest_bytes): (i64, Option<Vec<u8>>) = db
            .execute(move |conn| {
                conn.first(move || {
                    e::epochs
                        .select((
                            e::protocol_version,
                            c::checkpoints
                                .select(c::checkpoint_digest)
                                .filter(c::sequence_number.eq(0))
                                .single_value(),
                        ))
                        .order_by(e::epoch.desc())
                })
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch system details: {e}")))?;

        let native = NativeProtocolConfig::get_for_version_if_supported(
            protocol_version.unwrap_or(latest_version as u64).into(),
            ChainIdentifier::from_bytes(digest_bytes.unwrap_or_default())?.chain(),
        )
        .ok_or_else(|| {
            Error::ProtocolVersionUnsupported(
                ProtocolVersion::MIN.as_u64(),
                ProtocolVersion::MAX.as_u64(),
            )
        })?;

        Ok(ProtocolConfigs { native })
    }
}
