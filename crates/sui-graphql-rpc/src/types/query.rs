// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::{connection::Connection, *};

use super::{
    address::Address,
    base64::Base64,
    checkpoint::{Checkpoint, CheckpointId},
    epoch::Epoch,
    event::EventFilter,
    object::{Object, ObjectFilter},
    owner::ObjectOwner,
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
};
use crate::context_data::{context_ext::DataProviderContextExt, db_data_provider::PgManager};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Query {
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        ctx.data_provider().fetch_chain_id().await
    }

    // availableRange

    // dryRunTransactionBlock

    async fn owner(&self, ctx: &Context<'_>, address: SuiAddress) -> Result<Option<ObjectOwner>> {
        // Currently only an account address can own an object
        let o = ctx.data_provider().fetch_obj(address, None).await?;
        Ok(o.and_then(|q| q.owner)
            .map(|o| ObjectOwner::Address(Address { address: o })))
    }

    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        ctx.data_provider().fetch_obj(address, version).await
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address { address })
    }

    async fn epoch(&self, ctx: &Context<'_>, epoch_id: Option<u64>) -> Option<Epoch> {
        // Defaults to latest epoch
        let result = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch(epoch_id)
            .await?;
    }

    async fn checkpoint(&self, id: CheckpointId) -> Option<Checkpoint> {
        match (id.digest, id.sequence_number) {
            (Some(_), Some(_)) => None,
            _ => {
                let result = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_checkpoint(id)
                    .await?;
                Ok(result.map(|cp| cp.into()))
            }
        }
    }

    async fn transaction_block(
        &self,
        ctx: &Context<'_>,
        digest: String,
    ) -> Result<Option<TransactionBlock>> {
        // TODO: might benefit from having a mapping from StoredTransaction to graphql::TransactionBlock
        let result = ctx.data_unchecked::<PgManager>().fetch_tx(digest).await?;
        Ok(result.map(|tx| tx.into()))
    }

    // coinMetadata

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

    async fn transaction_block_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Connection<String, TransactionBlock>>> {
        if let Some(filter) = &filter {
            validate_package_dependencies(
                filter.package.as_ref(),
                filter.module.as_ref(),
                filter.function.as_ref(),
            )?;
        }

        let result = ctx
            .data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, filter)
            .await?;
        Ok(None)
    }

    async fn event_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<EventFilter>,
    ) -> Result<Option<Connection<String, Event>>> {
        ctx.data_provider()
            .fetch_event_connection(first, after, last, before)
            .await
    }

    async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: ObjectFilter,
    ) -> Result<Connection<String, Object>> {
        ctx.data_provider()
            .fetch_object_connection(first, after, last, before)
            .await
    }

    // resolveNameServiceAddress

    // allEpochAddressMetricsConnection

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

// validations
use crate::error::Error;
pub(crate) fn validate_package_dependencies(
    p: Option<&SuiAddress>,
    m: Option<&String>,
    ft: Option<&String>,
) -> Result<()> {
    if ft.is_some() && (p.is_none() || m.is_none()) {
        return Err(Error::RequiresModuleAndPackage.extend());
    }

    if m.is_some() && p.is_none() {
        return Err(Error::RequiresPackage.extend());
    }
    Ok(())
}
