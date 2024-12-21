// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{ExpressionMethods, QueryDsl};

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_types::sui_serde::BigInt;

use super::Reader;

#[rpc(server, namespace = "suix")]
trait Governance {
    /// Return the reference gas price for the network as of the latest epoch.
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>>;
}

pub(crate) struct GovernanceImpl(pub Reader);

#[async_trait::async_trait]
impl GovernanceServer for GovernanceImpl {
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        use kv_epoch_starts::dsl as e;

        let mut conn = self.0.connect().await?;
        let rgp: i64 = conn
            .first(
                e::kv_epoch_starts
                    .select(e::reference_gas_price)
                    .order(e::epoch.desc()),
            )
            .await?;

        Ok((rgp as u64).into())
    }
}
