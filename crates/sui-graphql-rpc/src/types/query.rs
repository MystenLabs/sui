// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    address::Address,
    checkpoint::{Checkpoint, CheckpointId},
    coin::CoinMetadata,
    epoch::Epoch,
    event::{Event, EventFilter},
    object::{Object, ObjectFilter},
    owner::{ObjectOwner, Owner},
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    sui_system_state_summary::SuiSystemStateSummary,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
};
use crate::{
    config::ServiceConfig,
    context_data::db_data_provider::PgManager,
    error::{code, graphql_error, Error},
};
use async_graphql::{connection::Connection, *};
use sui_json_rpc::{coin_api::parse_to_struct_tag, name_service::NameServiceConfig};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, EmptyMutation, EmptySubscription>;

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

    async fn owner(&self, address: SuiAddress) -> Option<ObjectOwner> {
        Some(ObjectOwner::Owner(Owner { address }))
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
            .fetch_checkpoints(first, after, last, before, None)
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
        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, filter)
            .await
            .extend()
    }

    async fn event_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: EventFilter,
    ) -> Result<Option<Connection<String, Event>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_events(first, after, last, before, filter)
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
        ctx.data_unchecked::<PgManager>()
            .fetch_protocol_configs(protocol_version)
            .await
            .extend()
    }

    /// Resolves the owner address of the provided domain name
    async fn resolve_name_service_address(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Option<Address>> {
        ctx.data_unchecked::<PgManager>()
            .resolve_name_service_address(ctx.data_unchecked::<NameServiceConfig>(), name)
            .await
            .extend()
    }

    async fn latest_sui_system_state(&self, ctx: &Context<'_>) -> Result<SuiSystemStateSummary> {
        ctx.data_unchecked::<PgManager>()
            .fetch_latest_sui_system_state()
            .await
            .extend()
    }

    async fn coin_metadata(
        &self,
        ctx: &Context<'_>,
        coin_type: String,
    ) -> Result<Option<CoinMetadata>> {
        let coin_struct = parse_to_struct_tag(&coin_type)?;
        let coin_metadata = ctx
            .data_unchecked::<PgManager>()
            .inner
            .get_coin_metadata_in_blocking_task(coin_struct.clone())
            .await
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;

        Ok(Some(CoinMetadata {
            decimals: coin_metadata.as_ref().map(|c| c.decimals),
            name: coin_metadata.as_ref().map(|c| c.name.clone()),
            symbol: coin_metadata.as_ref().map(|c| c.symbol.clone()),
            description: coin_metadata.as_ref().map(|c| c.description.clone()),
            icon_url: coin_metadata.as_ref().and_then(|c| c.icon_url.clone()),
            coin_type,
        }))
    }
}
