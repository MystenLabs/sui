// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::context_ext::DataProviderContextExt;
use crate::types::balance::*;
use crate::types::coin::*;
use crate::types::object::*;
use crate::types::stake::*;
use crate::types::sui_address::SuiAddress;
use async_graphql::connection::Connection;
use async_graphql::*;

use super::address::Address;
use super::name_service::NameService;

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
        ty = "Balance",
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
    field(
        name = "name_service_connection",
        ty = "Option<Connection<String, NameService>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>")
    )
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
    ) -> Result<Connection<String, Object>> {
        ctx.data_provider()
            .fetch_owned_objs(&self.address, first, after, last, before, filter)
            .await
    }

    pub async fn balance(&self, ctx: &Context<'_>, type_: Option<String>) -> Result<Balance> {
        ctx.data_provider()
            .fetch_balance(&self.address, type_)
            .await
    }

    pub async fn balance_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Balance>> {
        ctx.data_provider()
            .fetch_balance_connection(&self.address, first, after, last, before)
            .await
    }

    pub async fn coin_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Option<Connection<String, Coin>> {
        unimplemented!()
    }

    pub async fn stake_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<Connection<String, Stake>> {
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
    ) -> Option<Connection<String, NameService>> {
        unimplemented!()
    }
}
