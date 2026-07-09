// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_json_rpc_types::ProtocolConfigResponse;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;
use sui_types::sui_serde::BigInt;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::error::RpcError;
use crate::error::invalid_params;

#[open_rpc(namespace = "sui", tag = "Protocol API")]
#[rpc(server, namespace = "sui")]
trait ProtocolApi {
    /// Return the protocol config table for the given version number.
    /// If the version number is not specified, the protocol config for the latest indexed epoch is
    /// returned.
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
    #[error("Unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),
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
async fn protocol_config_response(
    ctx: &Context,
    version: Option<BigInt<u64>>,
) -> Result<ProtocolConfigResponse, RpcError<Error>> {
    let version = match version {
        Some(version) => *version,
        None => latest_protocol_version(ctx).await?,
    };

    let chain = ctx
        .chain_identifier()
        .context("Chain identifier not available")?
        .chain();

    let config =
        ProtocolConfig::get_for_version_if_supported(version.into(), chain).ok_or_else(|| {
            invalid_params(Error::ProtocolVersionUnsupported(
                ProtocolVersion::MIN.as_u64(),
                ProtocolVersion::MAX.as_u64(),
            ))
        })?;

    Ok(ProtocolConfigResponse::from(config))
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
