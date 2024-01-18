// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::{connection::Connection, *};
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::{gas_coin::GAS, TypeTag};

use super::{
    address::Address,
    available_range::AvailableRange,
    chain_identifier::ChainIdentifier,
    checkpoint::{self, Checkpoint, CheckpointId},
    coin::Coin,
    coin_metadata::CoinMetadata,
    cursor::Page,
    digest::Digest,
    epoch::Epoch,
    event::{self, Event, EventFilter},
    move_type::MoveType,
    object::{self, Object, ObjectFilter},
    owner::Owner,
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    suins_registration::{Domain, SuinsRegistration},
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
    type_filter::ExactTypeFilter,
};
use crate::{
    config::ServiceConfig, context_data::db_data_provider::PgManager, data::Db, error::Error,
    mutation::Mutation,
};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, Mutation, EmptySubscription>;

#[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the
    /// network).
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        Ok(ChainIdentifier::query(ctx.data_unchecked())
            .await
            .extend()?
            .to_string())
    }

    /// Range of checkpoints that the RPC has data available for (for data
    /// that can be tied to a particular checkpoint).
    async fn available_range(&self, ctx: &Context<'_>) -> Result<AvailableRange> {
        let (first, last) = ctx.data_unchecked::<PgManager>().available_range().await?;
        Ok(AvailableRange { first, last })
    }

    /// Configuration for this RPC service
    async fn service_config(&self, ctx: &Context<'_>) -> Result<ServiceConfig> {
        ctx.data()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .cloned()
            .extend()
    }

    // availableRange - pending impl. on IndexerV2
    // dryRunTransactionBlock
    // coinMetadata

    async fn owner(&self, address: SuiAddress) -> Option<Owner> {
        Some(Owner { address })
    }

    /// The object corresponding to the given address at the (optionally) given version.
    /// When no version is given, the latest version is returned.
    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        Object::query(ctx.data_unchecked(), address, version)
            .await
            .extend()
    }

    /// Look-up an Account by its SuiAddress.
    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address { address })
    }

    /// Fetch a structured representation of a concrete type, including its layout information.
    /// Fails if the type is malformed.
    async fn type_(&self, type_: String) -> Result<MoveType> {
        Ok(MoveType::new(
            TypeTag::from_str(&type_)
                .map_err(|e| Error::Client(format!("Bad type: {e}")))
                .extend()?,
        ))
    }

    /// Fetch epoch information by ID (defaults to the latest epoch).
    async fn epoch(&self, ctx: &Context<'_>, id: Option<u64>) -> Result<Option<Epoch>> {
        Epoch::query(ctx.data_unchecked(), id).await.extend()
    }

    /// Fetch checkpoint information by sequence number or digest (defaults to the latest available
    /// checkpoint).
    async fn checkpoint(
        &self,
        ctx: &Context<'_>,
        id: Option<CheckpointId>,
    ) -> Result<Option<Checkpoint>> {
        Checkpoint::query(ctx.data_unchecked(), id.unwrap_or_default())
            .await
            .extend()
    }

    /// Fetch a transaction block by its transaction digest.
    async fn transaction_block(
        &self,
        ctx: &Context<'_>,
        digest: Digest,
    ) -> Result<Option<TransactionBlock>> {
        TransactionBlock::query(ctx.data_unchecked(), digest)
            .await
            .extend()
    }

    /// The coin objects that exist in the network.
    ///
    /// The type field is a string of the inner type of the coin by which to filter (e.g.
    /// `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let coin = type_.map_or_else(GAS::type_tag, |t| t.0);
        Coin::paginate(ctx.data_unchecked(), page, coin, /* owner */ None)
            .await
            .extend()
    }

    /// The checkpoints that exist in the network.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<checkpoint::Cursor>,
        last: Option<u64>,
        before: Option<checkpoint::Cursor>,
    ) -> Result<Connection<String, Checkpoint>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Checkpoint::paginate(ctx.data_unchecked(), page, None)
            .await
            .extend()
    }

    /// The transaction blocks that exist in the network.
    async fn transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        TransactionBlock::paginate(ctx.data_unchecked(), page, filter.unwrap_or_default())
            .await
            .extend()
    }

    /// The events that exist in the network.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<event::Cursor>,
        last: Option<u64>,
        before: Option<event::Cursor>,
        filter: Option<EventFilter>,
    ) -> Result<Connection<String, Event>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Event::paginate(ctx.data_unchecked(), page, filter.unwrap_or_default())
            .await
            .extend()
    }

    /// The objects that exist in the network.
    async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Object::paginate(ctx.data_unchecked(), page, filter.unwrap_or_default())
            .await
            .extend()
    }

    /// Fetch the protocol config by protocol version (defaults to the latest protocol
    /// version known to the GraphQL service).
    async fn protocol_config(
        &self,
        ctx: &Context<'_>,
        protocol_version: Option<u64>,
    ) -> Result<ProtocolConfigs> {
        ProtocolConfigs::query(ctx.data_unchecked(), protocol_version)
            .await
            .extend()
    }

    /// Resolves a SuiNS `domain` name to an address, if it has been bound.
    async fn resolve_suins_address(
        &self,
        ctx: &Context<'_>,
        domain: Domain,
    ) -> Result<Option<Address>> {
        Ok(SuinsRegistration::resolve_to_record(
            ctx.data_unchecked::<Db>(),
            ctx.data_unchecked::<NameServiceConfig>(),
            &domain,
        )
        .await
        .extend()?
        .and_then(|r| r.target_address)
        .map(|a| Address { address: a.into() }))
    }

    /// The coin metadata associated with the given coin type.
    async fn coin_metadata(
        &self,
        ctx: &Context<'_>,
        coin_type: String,
    ) -> Result<Option<CoinMetadata>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_coin_metadata(coin_type)
            .await
            .extend()
    }
}
