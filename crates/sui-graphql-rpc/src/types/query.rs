// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};

use super::{
    address::Address,
    checkpoint::{Checkpoint, CheckpointId},
    epoch::Epoch,
    object::{Object, ObjectFilter},
    owner::ObjectOwner,
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
};
use crate::{
    config::ServiceConfig,
    context_data::{context_ext::DataProviderContextExt, db_data_provider::PgManager},
    error::{code, graphql_error, Error},
};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the
    /// network).
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        ctx.data_unchecked::<PgManager>()
            .fetch_chain_identifier()
            .await
            .extend()
    }

    /// Configuration for this RPC service
    async fn service_config(&self, ctx: &Context<'_>) -> Result<ServiceConfig> {
        Ok(ctx
            .data()
            .map_err(|_| {
                graphql_error(
                    code::INTERNAL_SERVER_ERROR,
                    "Unable to fetch service configuration",
                )
            })
            .cloned()?)
    }

    // availableRange - pending impl. on IndexerV2
    // dryRunTransactionBlock
    // coinMetadata
    // resolveNameServiceAddress
    // event_connection -> TODO: need to define typings

    async fn owner(&self, ctx: &Context<'_>, address: SuiAddress) -> Result<Option<ObjectOwner>> {
        // Currently only an account address can own an object
        let owner_address = ctx
            .data_unchecked::<PgManager>()
            .fetch_owner(address)
            .await?;

        Ok(owner_address.map(|o| ObjectOwner::Address(Address { address: o })))
    }

    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_obj(address, version)
            .await
            .extend()
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address { address })
    }

    async fn epoch(&self, ctx: &Context<'_>, id: Option<u64>) -> Result<Option<Epoch>> {
        if let Some(epoch_id) = id {
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch(epoch_id)
                .await
                .extend()
        } else {
            Some(
                ctx.data_unchecked::<PgManager>()
                    .fetch_latest_epoch()
                    .await
                    .extend(),
            )
            .transpose()
        }
    }

    async fn checkpoint(
        &self,
        ctx: &Context<'_>,
        id: Option<CheckpointId>,
    ) -> Result<Option<Checkpoint>> {
        if let Some(id) = id {
            match (&id.digest, &id.sequence_number) {
                (Some(_), Some(_)) => Err(Error::InvalidCheckpointQuery.extend()),
                _ => ctx
                    .data_unchecked::<PgManager>()
                    .fetch_checkpoint(id.digest.as_deref(), id.sequence_number)
                    .await
                    .extend(),
            }
        } else {
            Some(
                ctx.data_unchecked::<PgManager>()
                    .fetch_latest_checkpoint()
                    .await
                    .extend(),
            )
            .transpose()
        }
    }

    async fn transaction_block(
        &self,
        ctx: &Context<'_>,
        digest: String,
    ) -> Result<Option<TransactionBlock>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_tx(&digest)
            .await
            .extend()
    }

    async fn checkpoint_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Checkpoint>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoints(first, after, last, before)
            .await
            .extend()
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

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, filter)
            .await
            .extend()
    }

    async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>> {
        if let Some(filter) = &filter {
            validate_package_dependencies(
                filter.package.as_ref(),
                filter.module.as_ref(),
                filter.ty.as_ref(),
            )?;
        }

        ctx.data_unchecked::<PgManager>()
            .fetch_objs(first, after, last, before, filter)
            .await
            .extend()
    }

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
