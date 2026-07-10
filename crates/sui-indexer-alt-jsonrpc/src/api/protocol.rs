// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde_json::Value;
use sui_indexer_alt_schema::epochs::StoredFeatureFlag;
use sui_indexer_alt_schema::epochs::StoredProtocolConfig;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_indexer_alt_schema::schema::kv_feature_flags;
use sui_indexer_alt_schema::schema::kv_protocol_configs;
use sui_json_rpc_types::ProtocolConfigResponse;
use sui_json_rpc_types::SuiProtocolConfigValue;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_protocol_config::ProtocolVersion;
use sui_types::sui_serde::BigInt;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::error::RpcError;
use crate::error::invalid_params;

#[open_rpc(namespace = "sui", tag = "Protocol API")]
#[rpc(server, namespace = "sui")]
trait ProtocolApi {
    /// Return the protocol config table for the given version number. If the version number is not
    /// specified, the protocol config for the latest indexed epoch is returned.
    #[method(name = "getProtocolConfig")]
    async fn get_protocol_config(
        &self,
        /// An optional protocol version specifier. If omitted, the protocol config for the latest indexed epoch will be returned.
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse>;
}

pub(crate) struct Protocol(pub Context);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Protocol version {0} not found")]
    ProtocolVersionNotFound(u64),
}

#[async_trait::async_trait]
impl ProtocolApiServer for Protocol {
    async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse> {
        Ok(protocol_config_response(&self.0, version).await?)
    }
}

impl RpcModule for Protocol {
    fn schema(&self) -> Module {
        ProtocolApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

/// Load data and generate response for `getProtocolConfig`.
///
/// The response is built from the `kv_protocol_configs` and `kv_feature_flags` tables (which the
/// indexer materializes at each epoch boundary), rather than from this binary's own
/// `ProtocolConfig`, so that the RPC keeps serving correct values when the chain's protocol version
/// is newer than the binary.
///
/// The tables store values stringified, which costs some fidelity compared to the legacy response:
/// only protocol versions the indexer has seen can be served; `attributes` reports every integer as
/// `u64` (declared widths are not preserved); and in `configs`, all numbers are JSON strings
/// (legacy rendered u16/u32 fields as JSON numbers) and non-scalar values are absent.
async fn protocol_config_response(
    ctx: &Context,
    version: Option<BigInt<u64>>,
) -> Result<ProtocolConfigResponse, RpcError<Error>> {
    use kv_feature_flags::dsl as f;
    use kv_protocol_configs::dsl as p;

    let version = match version {
        Some(version) => *version,
        None => latest_protocol_version(ctx).await?,
    };

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let stored_configs: Vec<StoredProtocolConfig> = conn
        .results(p::kv_protocol_configs.filter(p::protocol_version.eq(version as i64)))
        .await
        .context("Failed to fetch protocol configs")?;

    let stored_flags: Vec<StoredFeatureFlag> = conn
        .results(f::kv_feature_flags.filter(f::protocol_version.eq(version as i64)))
        .await
        .context("Failed to fetch feature flags")?;

    if stored_configs.is_empty() && stored_flags.is_empty() {
        return Err(invalid_params(Error::ProtocolVersionNotFound(version)));
    }

    let feature_flags: BTreeMap<String, bool> = stored_flags
        .into_iter()
        .map(|flag| (flag.flag_name, flag.flag_value))
        .collect();

    // `configs` is the complete view of the protocol config: attributes that are set at this
    // version, merged with the feature flags.
    let mut attributes = BTreeMap::new();
    let mut configs = BTreeMap::new();
    for config in stored_configs {
        if let Some(value) = &config.config_value {
            configs.insert(config.config_name.clone(), raw_config_to_json(value));
        }
        let attribute = config.config_value.as_deref().and_then(parse_attribute);
        attributes.insert(config.config_name, attribute);
    }

    for (flag, value) in &feature_flags {
        configs.insert(flag.clone(), Value::Bool(*value));
    }

    Ok(ProtocolConfigResponse {
        // Min and max supported versions are properties of a binary -- these are the versions this
        // RPC's own software knows about, which may trail the version being served.
        min_supported_protocol_version: ProtocolVersion::MIN,
        max_supported_protocol_version: ProtocolVersion::MAX,
        protocol_version: version.into(),
        feature_flags,
        attributes,
        configs,
    })
}

/// Fetch the protocol version of the latest epoch from the database.
async fn latest_protocol_version(ctx: &Context) -> Result<u64, RpcError<Error>> {
    use kv_epoch_starts::dsl as e;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let version: i64 = conn
        .first(
            e::kv_epoch_starts
                .select(e::protocol_version)
                .order(e::epoch.desc()),
        )
        .await
        .context("Failed to fetch the latest protocol version")?;

    Ok(version as u64)
}

/// Recover a typed attribute value from its stored string representation. The database does not
/// preserve integer widths, so all integer values parse as `u64`.
fn parse_attribute(value: &str) -> Option<SuiProtocolConfigValue> {
    match value {
        "true" => Some(SuiProtocolConfigValue::Bool(true)),
        "false" => Some(SuiProtocolConfigValue::Bool(false)),
        _ => value.parse().ok().map(SuiProtocolConfigValue::U64),
    }
}

/// Convert a stored config value to its JSON representation: booleans as booleans, everything
/// else as a string. Numbers are uniformly strings (rather than following the legacy convention
/// of rendering u16/u32 as JSON numbers) because the stored value does not carry its declared
/// width, and inferring it from the magnitude would make a config's JSON type change when its
/// value grows.
fn raw_config_to_json(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(value.to_owned()),
    }
}
