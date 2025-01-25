// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{ExpressionMethods, QueryDsl};

use jsonrpsee::{
    core::{DeserializeOwned, RpcResult},
    proc_macros::rpc,
};
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::ObjectID,
    dynamic_field::{derive_dynamic_field_id, Field},
    object::Object,
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
    data::{object_versions::LatestObjectKey, objects::ObjectVersionKey},
    error::{internal_error, rpc_bail},
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
        use kv_epoch_starts::dsl as e;

        let Self(ctx) = self;
        let mut conn = ctx.reader().connect().await.map_err(internal_error)?;
        let rgp: i64 = conn
            .first(
                e::kv_epoch_starts
                    .select(e::reference_gas_price)
                    .order(e::epoch.desc()),
            )
            .await
            .map_err(internal_error)?;

        Ok((rgp as u64).into())
    }

    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        let Self(ctx) = self;

        /// Fetch the latest version of the object at ID `object_id`, and deserialize its contents
        /// as a Rust type `T`, assuming that it is a Move object (not a package).
        ///
        /// This function extracts the common parts of object loading for this API, but it does not
        /// generalize beyond that, because:
        ///
        /// - It assumes that the objects being loaded are never deleted or wrapped (because it
        ///   loads using `LatestObjectKey` directly without checking the live object set).
        ///
        /// - It first fetches one record from `obj_versions` and then fetches its contents. It is
        ///   easy to misuse this API to fetch multiple objects in sequence, in a loop, rather than
        ///   fetching them concurrently.
        async fn fetch_latest<T: DeserializeOwned>(
            ctx: &Context,
            object_id: ObjectID,
        ) -> Result<T, String> {
            let id_display = object_id.to_canonical_display(/* with_prefix */ true);
            let loader = ctx.loader();

            let latest_version = loader
                .load_one(LatestObjectKey(object_id))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Failed to load latest version for {id_display}"))?;

            let stored = loader
                .load_one(ObjectVersionKey(
                    object_id,
                    latest_version.object_version as u64,
                ))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Failed to load object for {id_display}"))?
                .serialized_object
                .ok_or_else(|| format!("Failed to load contents for {id_display}"))?;

            let object: Object = bcs::from_bytes(&stored).map_err(|e| e.to_string())?;

            let move_object = object
                .data
                .try_as_move()
                .ok_or_else(|| format!("{id_display} is not a Move object"))?;

            bcs::from_bytes(move_object.contents())
                .map_err(|e| format!("Failed to deserialize contents for {id_display}: {e}"))
        }

        let wrapper: SuiSystemStateWrapper = fetch_latest(ctx, SUI_SYSTEM_STATE_OBJECT_ID)
            .await
            .map_err(internal_error)?;

        let inner_id = derive_dynamic_field_id(
            SUI_SYSTEM_STATE_OBJECT_ID,
            &TypeTag::U64,
            &bcs::to_bytes(&wrapper.version).map_err(internal_error)?,
        )
        .map_err(internal_error)?;

        Ok(match wrapper.version {
            1 => fetch_latest::<Field<u64, SuiSystemStateInnerV1>>(ctx, inner_id)
                .await
                .map_err(internal_error)?
                .value
                .into_sui_system_state_summary(),
            2 => fetch_latest::<Field<u64, SuiSystemStateInnerV2>>(ctx, inner_id)
                .await
                .map_err(internal_error)?
                .value
                .into_sui_system_state_summary(),
            v => rpc_bail!(internal_error("Unexpected inner system state version: {v}")),
        })
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
