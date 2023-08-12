// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::data_provider::fetch_balance;
use crate::server::data_provider::fetch_owned_objs;
use crate::types::balance::*;
use crate::types::coin::*;
use crate::types::name_service::*;
use crate::types::object::*;
use crate::types::stake::*;
use crate::types::sui_address::SuiAddress;
use async_graphql::connection::Connection;
use async_graphql::*;

use super::address::Address;

#[derive(Interface)]
#[graphql(
    field(name = "location", type = "SuiAddress"),
    field(
        name = "object_connection",
        type = "Option<Connection<String, Object>>",
        arg(name = "first", type = "Option<u64>"),
        arg(name = "after", type = "Option<String>"),
        arg(name = "last", type = "Option<u64>"),
        arg(name = "before", type = "Option<String>"),
        arg(name = "filter", type = "Option<ObjectFilter>")
    ),
    field(
        name = "balance",
        type = "Balance",
        arg(name = "type", type = "Option<String>")
    ),
    field(
        name = "balance_connection",
        type = "Option<BalanceConnection>",
        arg(name = "first", type = "Option<u64>"),
        arg(name = "after", type = "Option<String>"),
        arg(name = "last", type = "Option<u64>"),
        arg(name = "before", type = "Option<String>")
    ),
    field(
        name = "coin_connection",
        type = "Option<CoinConnection>",
        arg(name = "first", type = "Option<u64>"),
        arg(name = "after", type = "Option<String>"),
        arg(name = "last", type = "Option<u64>"),
        arg(name = "before", type = "Option<String>"),
        arg(name = "type", type = "Option<String>")
    ),
    field(
        name = "stake_connection",
        type = "Option<StakeConnection>",
        arg(name = "first", type = "Option<u64>"),
        arg(name = "after", type = "Option<String>"),
        arg(name = "last", type = "Option<u64>"),
        arg(name = "before", type = "Option<String>")
    ),
    field(name = "default_name_service_name", type = "Option<String>"),
    field(
        name = "name_service_connection",
        type = "Option<NameServiceConnection>",
        arg(name = "first", type = "Option<u64>"),
        arg(name = "after", type = "Option<String>"),
        arg(name = "last", type = "Option<u64>"),
        arg(name = "before", type = "Option<String>")
    )
)]
pub(crate) enum ObjectOwner {
    Address(Address),
    Owner(Owner),
    Object(Object),
}

pub(crate) struct Owner {
    pub address: SuiAddress,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Owner {
    async fn as_address(&self, ctx: &Context<'_>) -> Option<Address> {
        // For now only addresses can be owners
        Some(Address {
            address: self.address.clone(),
        })
    }

    async fn as_object(&self, ctx: &Context<'_>) -> Option<Object> {
        // TODO: extend when send to object imnplementation is done
        // For now only addresses can be owners
        None
    }

    // =========== Owner interface methods =============

    pub async fn location(&self, ctx: &Context<'_>) -> SuiAddress {
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
