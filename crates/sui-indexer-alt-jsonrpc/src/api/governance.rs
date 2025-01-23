// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{ExpressionMethods, QueryDsl};

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::sui_serde::BigInt;

use crate::{context::Context, error::internal_error};

use super::rpc_module::RpcModule;

#[open_rpc(namespace = "suix", tag = "Governance API")]
#[rpc(server, namespace = "suix")]
trait GovernanceApi {
    /// Return the reference gas price for the network as of the latest epoch.
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>>;
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
}

impl RpcModule for Governance {
    fn schema(&self) -> Module {
        GovernanceApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
