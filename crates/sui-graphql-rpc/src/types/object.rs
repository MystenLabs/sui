// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};

use super::{
    balance::{Balance, BalanceConnection},
    coin::CoinConnection,
    name_service::NameServiceConnection,
    stake::StakeConnection,
    sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};
use crate::{
    server::data_provider::{fetch_balance, fetch_owned_objs, fetch_tx},
    types::base64::Base64,
};

pub(crate) struct Object {
    pub address: SuiAddress,
    pub version: u64,
    pub digest: String,
    pub storage_rebate: Option<u64>,
    pub owner: Option<SuiAddress>,
    pub bcs: Option<Base64>,
    pub previous_transaction: Option<String>,
}

#[derive(InputObject)]
pub(crate) struct ObjectFilter {
    package: Option<SuiAddress>,
    module: Option<String>,
    ty: Option<String>,

    owner: Option<SuiAddress>,
    object_id: Option<SuiAddress>,
    version: Option<u64>,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Object {
    async fn version(&self) -> u64 {
        self.version
    }

    async fn digest(&self) -> String {
        self.digest.clone()
    }

    async fn storage_rebate(&self) -> Option<u64> {
        self.storage_rebate
    }

    async fn bcs(&self) -> Option<Base64> {
        self.bcs.clone()
    }

    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        if let Some(tx) = &self.previous_transaction {
            fetch_tx(ctx.data_unchecked::<sui_sdk::SuiClient>(), tx).await
        } else {
            Ok(None)
        }
    }

    // =========== Owner interface methods =============

    pub async fn location(&self) -> SuiAddress {
        self.address.clone()
    }

    pub async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        fetch_owned_objs(
            ctx.data_unchecked::<sui_sdk::SuiClient>(),
            &self.address,
            first,
            after,
            last,
            before,
            filter,
        )
        .await
    }

    pub async fn balance(&self, ctx: &Context<'_>, type_: Option<String>) -> Result<Balance> {
        fetch_balance(
            ctx.data_unchecked::<sui_sdk::SuiClient>(),
            &self.address,
            type_,
        )
        .await
    }

    pub async fn balance_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<BalanceConnection> {
        unimplemented!()
    }

    pub async fn coin_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Option<CoinConnection> {
        unimplemented!()
    }

    pub async fn stake_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<StakeConnection> {
        unimplemented!()
    }

    pub async fn default_name_service_name(&self) -> Option<String> {
        unimplemented!()
    }

    pub async fn name_service_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<NameServiceConnection> {
        unimplemented!()
    }
}
