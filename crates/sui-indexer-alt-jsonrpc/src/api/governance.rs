// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::{ExpressionMethods, QueryDsl};

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    dynamic_field::{derive_dynamic_field_id, Field},
    sui_serde::BigInt,
    sui_system_state::{
        sui_system_state_inner_v1::SuiSystemStateInnerV1,
        sui_system_state_inner_v2::SuiSystemStateInnerV2,
        sui_system_state_summary::SuiSystemStateSummary, SuiSystemStateTrait,
        SuiSystemStateWrapper,
    },
    TypeTag, SUI_SYSTEM_STATE_OBJECT_ID,
};

use crate::{
    context::Context,
    data::objects::load_latest_deserialized,
    error::{rpc_bail, RpcError},
};

use super::rpc_module::RpcModule;

#[open_rpc(namespace = "suix", tag = "Governance API")]
#[rpc(server, namespace = "suix")]
trait GovernanceApi {
    /// Return the reference gas price for the network as of the latest epoch.
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>>;

    /// Return a summary of the latest version of the Sui System State object (0x5), on-chain.
    #[method(name = "getLatestSuiSystemState")]
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary>;
}

pub(crate) struct Governance(pub Context);

#[async_trait::async_trait]
impl GovernanceApiServer for Governance {
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        Ok(rgp_response(&self.0).await?)
    }

    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        Ok(latest_sui_system_state_response(&self.0).await?)
    }
}

impl RpcModule for Governance {
    fn schema(&self) -> Module {
        GovernanceApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

/// Load data and generate response for `getReferenceGasPrice`.
async fn rgp_response(ctx: &Context) -> Result<BigInt<u64>, RpcError> {
    use kv_epoch_starts::dsl as e;

    let mut conn = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let rgp: i64 = conn
        .first(
            e::kv_epoch_starts
                .select(e::reference_gas_price)
                .order(e::epoch.desc()),
        )
        .await
        .context("Failed to fetch the reference gas price")?;

    Ok((rgp as u64).into())
}

/// Load data and generate response for `getLatestSuiSystemState`.
async fn latest_sui_system_state_response(
    ctx: &Context,
) -> Result<SuiSystemStateSummary, RpcError> {
    let loader = ctx.loader();
    let wrapper: SuiSystemStateWrapper =
        load_latest_deserialized(loader, SUI_SYSTEM_STATE_OBJECT_ID)
            .await
            .context("Failed to fetch system state wrapper object")?;

    let inner_id = derive_dynamic_field_id(
        SUI_SYSTEM_STATE_OBJECT_ID,
        &TypeTag::U64,
        &bcs::to_bytes(&wrapper.version).context("Failed to serialize system state version")?,
    )
    .context("Failed to derive inner system state field ID")?;

    Ok(match wrapper.version {
        1 => load_latest_deserialized::<Field<u64, SuiSystemStateInnerV1>>(loader, inner_id)
            .await
            .context("Failed to fetch inner system state object")?
            .value
            .into_sui_system_state_summary(),
        2 => load_latest_deserialized::<Field<u64, SuiSystemStateInnerV2>>(loader, inner_id)
            .await
            .context("Failed to fetch inner system state object")?
            .value
            .into_sui_system_state_summary(),
        v => rpc_bail!("Unexpected inner system state version: {v}"),
    })
}
