// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::gas_coin::GAS;

use crate::{data::Db, error::Error};

use super::{
    balance::{self, Balance},
    coin::Coin,
    cursor::Page,
    dynamic_field::{DynamicField, DynamicFieldName},
    object::{self, Object, ObjectFilter},
    stake::StakedSui,
    sui_address::SuiAddress,
    suins_registration::SuinsRegistration,
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
    type_filter::ExactTypeFilter,
};

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub(crate) struct Address {
    pub address: SuiAddress,
}

/// The possible relationship types for a transaction block: sign, sent, received, or paid.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum AddressTransactionBlockRelationship {
    /// Transactions this address has signed either as a sender or as a sponsor.
    Sign,
    /// Transactions that sent objects to this address.
    Recv,
}

/// The 32-byte address that is an account address (corresponding to a public key).
#[Object]
impl Address {
    /// Similar behavior to the `transactionBlocks` in Query but supporting the additional
    /// `AddressTransactionBlockRelationship` filter, which defaults to `SIGN`.
    async fn transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        relation: Option<AddressTransactionBlockRelationship>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        use AddressTransactionBlockRelationship as R;
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(match relation {
            // Relationship defaults to "signer" if none is supplied.
            Some(R::Sign) | None => TransactionBlockFilter {
                sign_address: Some(self.address),
                ..Default::default()
            },

            Some(R::Recv) => TransactionBlockFilter {
                recv_address: Some(self.address),
                ..Default::default()
            },
        }) else {
            return Ok(Connection::new(false, false));
        };

        TransactionBlock::paginate(ctx.data_unchecked(), page, filter)
            .await
            .extend()
    }

    // =========== Owner interface methods =============

    pub async fn address(&self) -> SuiAddress {
        self.address
    }

    /// The objects that are owned by this address.
    pub async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
            owner: Some(self.address),
            ..Default::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        Object::paginate(ctx.data_unchecked(), page, filter)
            .await
            .extend()
    }

    /// The balance that this address holds.
    pub async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        let coin = type_.map_or_else(GAS::type_tag, |t| t.0);
        Balance::query(ctx.data_unchecked(), self.address, coin)
            .await
            .extend()
    }

    /// The balances of all coin types owned by this address. Coins of the same type are grouped
    /// together into one Balance.
    pub async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Balance::paginate(ctx.data_unchecked(), page, self.address)
            .await
            .extend()
    }

    /// The coin objects for this address.
    ///
    /// The type field is a string of the inner type of the coin by which to filter (e.g.
    /// `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    pub async fn coins(
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
        Coin::paginate(ctx.data_unchecked(), page, coin, Some(self.address))
            .await
            .extend()
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this address.
    pub async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        StakedSui::paginate(ctx.data_unchecked(), page, self.address)
            .await
            .extend()
    }

    /// The domain that a user address has explicitly configured as their default domain.
    pub async fn default_suins_name(&self, ctx: &Context<'_>) -> Result<Option<String>> {
        Ok(SuinsRegistration::reverse_resolve_to_name(
            ctx.data_unchecked::<Db>(),
            ctx.data_unchecked::<NameServiceConfig>(),
            self.address,
        )
        .await
        .extend()?
        .map(|d| d.to_string()))
    }

    /// The SuinsRegistration NFTs owned by this address. These grant the owner the capability to
    /// manage the associated domain.
    pub async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        SuinsRegistration::paginate(
            ctx.data_unchecked::<Db>(),
            ctx.data_unchecked::<NameServiceConfig>(),
            page,
            self.address,
        )
        .await
        .extend()
    }

    /// This resolver is not supported on the Address type.
    pub async fn dynamic_field(&self, _name: DynamicFieldName) -> Result<Option<DynamicField>> {
        Err(Error::DynamicFieldOnAddress.extend())
    }

    /// This resolver is not supported on the Address type.
    pub async fn dynamic_object_field(
        &self,
        _name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        Err(Error::DynamicFieldOnAddress.extend())
    }

    pub async fn dynamic_fields(
        &self,
        _first: Option<u64>,
        _after: Option<object::Cursor>,
        _last: Option<u64>,
        _before: Option<object::Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        Err(Error::DynamicFieldOnAddress.extend())
    }
}
