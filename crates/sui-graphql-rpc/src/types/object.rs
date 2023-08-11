// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::{
    balance::{Balance, BalanceConnection},
    coin::CoinConnection,
    name_service::NameServiceConnection,
    stake::StakeConnection,
    sui_address::SuiAddress,
};

pub(crate) struct Object;
pub(crate) struct ObjectConnection;

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
    // =========== Owner interface methods =============

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

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl ObjectConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
