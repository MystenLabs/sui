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

    async fn owner(&self, ctx: &Context<'_>, address: SuiAddress) -> Result<Option<Owner>> {
        // Currently only an account address can own an object
        let cl = ctx.data_unchecked::<sui_sdk::SuiClient>();
        let o = crate::server::data_provider::fetch_obj(cl, address, None).await?;
        Ok(o.and_then(|q| q.owner)
            .map(|o| Owner::Address(Address { address: o })))
    }

    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        let cl = ctx.data_unchecked::<sui_sdk::SuiClient>();
        crate::server::data_provider::fetch_obj(cl, address, version).await
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address {
            address: address.clone(),
        })
    }
}
