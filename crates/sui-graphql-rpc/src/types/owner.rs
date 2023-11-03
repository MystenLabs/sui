// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::address::Address;
use super::stake::Stake;
use crate::context_data::db_data_provider::PgManager;
use crate::types::balance::*;
use crate::types::coin::*;
use crate::types::object::*;
use crate::types::sui_address::SuiAddress;

use async_graphql::connection::Connection;
use async_graphql::*;
use sui_json_rpc::name_service::NameServiceConfig;

#[derive(Interface)]
#[graphql(
    field(name = "location", ty = "SuiAddress"),
    field(
        name = "object_connection",
        ty = "Option<Connection<String, Object>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>"),
        arg(name = "filter", ty = "Option<ObjectFilter>")
    ),
    field(
        name = "balance",
        ty = "Option<Balance>",
        arg(name = "type", ty = "Option<String>")
    ),
    field(
        name = "balance_connection",
        ty = "Option<Connection<String, Balance>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>")
    ),
    field(
        name = "coin_connection",
        ty = "Option<Connection<String, Coin>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>"),
        arg(name = "type", ty = "Option<String>")
    ),
    field(
        name = "stake_connection",
        ty = "Option<Connection<String, Stake>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>")
    ),
    field(name = "default_name_service_name", ty = "Option<String>"),
    // TODO disabled-for-rpc-1.5
    // field(
    //     name = "name_service_connection",
    //     ty = "Option<Connection<String, NameService>>",
    //     arg(name = "first", ty = "Option<u64>"),
    //     arg(name = "after", ty = "Option<String>"),
    //     arg(name = "last", ty = "Option<u64>"),
    //     arg(name = "before", ty = "Option<String>")
    // )
)]
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum ObjectOwner {
    Address(Address),
    Owner(Owner),
    Object(Object),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct Owner {
    pub address: SuiAddress,
}

#[allow(clippy::diverging_sub_expression)]
#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Owner {
    async fn as_address(&self, ctx: &Context<'_>) -> Option<Address> {
        // For now only addresses can be owners
        Some(Address {
            address: self.address,
        })
    }

    async fn as_object(&self, ctx: &Context<'_>) -> Option<Object> {
        // TODO: extend when send to object imnplementation is done
        // For now only addresses can be owners
        None
    }

    // =========== Owner interface methods =============

    pub async fn location(&self, ctx: &Context<'_>) -> SuiAddress {
        self.address
    }

    pub async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_owned_objs(first, after, last, before, filter, self.address)
            .await
            .extend()
    }

    pub async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<String>,
    ) -> Result<Option<Balance>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balance(self.address, type_)
            .await
            .extend()
    }

    pub async fn balance_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Balance>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balances(self.address, first, after, last, before)
            .await
            .extend()
    }

    pub async fn coin_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Result<Option<Connection<String, Coin>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_coins(self.address, type_, first, after, last, before)
            .await
            .extend()
    }

    /// The stake objects for the given address
    pub async fn stake_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Stake>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_staked_sui(self.address, first, after, last, before)
            .await
            .extend()
    }

    pub async fn default_name_service_name(&self, ctx: &Context<'_>) -> Result<Option<String>> {
        ctx.data_unchecked::<PgManager>()
            .default_name_service_name(ctx.data_unchecked::<NameServiceConfig>(), self.address)
            .await
            .extend()
    }

    // TODO disabled-for-rpc-1.5
    // pub async fn name_service_connection(
    //     &self,
    //     ctx: &Context<'_>,
    //     first: Option<u64>,
    //     after: Option<String>,
    //     last: Option<u64>,
    //     before: Option<String>,
    // ) -> Result<Option<Connection<String, NameService>>> {
    //     unimplemented!()
    // }
}
