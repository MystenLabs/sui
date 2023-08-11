// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::{address::Address, object::Object, owner::Owner, sui_address::SuiAddress};

pub(crate) struct Query;

pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Query {
    async fn chain_identifier(&self) -> String {
        "0000".to_string()
    }

    async fn owner(&self, address: SuiAddress) -> Option<Owner> {
        None
    }

    async fn object(&self, address: SuiAddress, version: Option<u64>) -> Option<Object> {
        None
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        None
    }
}
