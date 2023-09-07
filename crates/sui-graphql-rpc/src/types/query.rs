// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    address::Address, checkpoint::Checkpoint, object::Object, owner::ObjectOwner,
    protocol_config::ProtocolConfigs, sui_address::SuiAddress,
};
use crate::{
    limits::complexity::{connection_calc as cc, get_complexity_config as gc, standard_calc as sc},
    server::context_ext::DataProviderContextExt,
};
use async_graphql::{connection::Connection, *};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Query {
    #[graphql(complexity = "sc(&gc().chain_identifier, child_complexity)")]
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        ctx.data_provider().fetch_chain_id().await
    }

    #[graphql(complexity = "sc(&gc().owner, child_complexity)")]
    async fn owner(&self, ctx: &Context<'_>, address: SuiAddress) -> Result<Option<ObjectOwner>> {
        // Currently only an account address can own an object
        let o = ctx.data_provider().fetch_obj(address, None).await?;
        Ok(o.and_then(|q| q.owner)
            .map(|o| ObjectOwner::Address(Address { address: o })))
    }

    #[graphql(complexity = "sc(&gc().object, child_complexity)")]
    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        ctx.data_provider().fetch_obj(address, version).await
    }

    #[graphql(complexity = "sc(&gc().address, child_complexity)")]
    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address { address })
    }

    #[graphql(complexity = "cc(&gc().checkpoint_connection, first, last, child_complexity)")]
    async fn checkpoint_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Checkpoint>> {
        ctx.data_provider()
            .fetch_checkpoint_connection(first, after, last, before)
            .await
    }

    #[graphql(complexity = "sc(&gc().protocol_config, child_complexity)")]
    async fn protocol_config(
        &self,
        ctx: &Context<'_>,
        protocol_version: Option<u64>,
    ) -> Result<ProtocolConfigs> {
        ctx.data_provider()
            .fetch_protocol_config(protocol_version)
            .await
    }
}
