// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::balance::*;
use crate::types::coin::*;
use crate::types::name_service::*;
use crate::types::object::*;
use crate::types::stake::*;
use crate::types::sui_address::SuiAddress;
use async_graphql::*;

use super::address::Address;

#[derive(Interface)]
#[graphql(
    field(name = "location", type = "SuiAddress"),
    field(
        name = "object_connection",
        type = "Option<ObjectConnection>",
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
pub(crate) enum Owner {
    Address(Address),
    AmbiguousOwner(AmbiguousOwner),
    Object(Object),
}

pub(crate) struct AmbiguousOwner;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl AmbiguousOwner {
    pub async fn location(&self) -> SuiAddress {
        unimplemented!()
    }

    pub async fn object_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Option<ObjectConnection> {
        unimplemented!()
    }

    pub async fn balance(&self, type_: Option<String>) -> Balance {
        unimplemented!()
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
