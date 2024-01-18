// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::address::Address;
use super::cursor::Page;
use super::dynamic_field::DynamicField;
use super::dynamic_field::DynamicFieldName;
use super::stake::StakedSui;
use super::suins_registration::SuinsRegistration;
use crate::context_data::db_data_provider::PgManager;
use crate::data::Db;
use crate::types::balance::*;
use crate::types::coin::*;
use crate::types::object::{self, *};
use crate::types::sui_address::SuiAddress;
use crate::types::type_filter::ExactTypeFilter;

use async_graphql::connection::Connection;
use async_graphql::*;
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::gas_coin::GAS;

#[derive(Interface)]
#[graphql(
    field(name = "address", ty = "SuiAddress"),
    field(
        name = "objects",
        ty = "Connection<String, Object>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        arg(name = "filter", ty = "Option<ObjectFilter>")
    ),
    field(
        name = "balance",
        ty = "Option<Balance>",
        arg(name = "type", ty = "Option<String>")
    ),
    field(
        name = "balance_connection",
        ty = "Option<Connection<String, Balance>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>")
    ),
    field(
        name = "coins",
        ty = "Connection<String, Coin>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        arg(name = "type", ty = "Option<ExactTypeFilter>")
    ),
    field(
        name = "staked_sui_connection",
        ty = "Option<Connection<String, StakedSui>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>")
    ),
    field(name = "default_suins_name", ty = "Option<String>"),
    field(
        name = "suins_registrations",
        ty = "Option<Connection<String, SuinsRegistration>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>")
    ),
    field(
        name = "dynamic_field",
        ty = "Option<DynamicField>",
        arg(name = "name", ty = "DynamicFieldName")
    ),
    field(
        name = "dynamic_object_field",
        ty = "Option<DynamicField>",
        arg(name = "name", ty = "DynamicFieldName")
    ),
    field(
        name = "dynamic_field_connection",
        ty = "Option<Connection<String, DynamicField>>",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<String>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<String>"),
    )
)]
#[derive(Clone, Debug)]
#[graphql(name = "IOwner")]
pub(crate) enum IOwner {
    Address(Address),
    Owner(Owner),
    Object(Object),
}

#[derive(Clone, Debug)]
pub(crate) struct Owner {
    pub address: SuiAddress,
}

/// An Owner is an entity that can own an object. Each Owner is identified by a SuiAddress which
/// represents either an Address (corresponding to a public key of an account) or an Object, but
/// never both (it is not known up-front whether a given Owner is an Address or an Object).
#[Object]
impl Owner {
    async fn as_address(&self) -> Option<Address> {
        // For now only addresses can be owners
        Some(Address {
            address: self.address,
        })
    }

    async fn as_object(&self) -> Option<Object> {
        // TODO: extend when send to object imnplementation is done
        // For now only addresses can be owners
        None
    }

    // =========== Owner interface methods =============

    pub async fn address(&self) -> SuiAddress {
        self.address
    }

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

        Object::paginate(ctx.data_unchecked(), page, None, filter)
            .await
            .extend()
    }

    /// Total balance of all coins with marker type owned by this Owner. If type is not supplied,
    /// it defaults to 0x2::sui::SUI.
    pub async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<String>,
    ) -> Result<Option<Balance>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balance(self.address, type_)
            .await
            .extend()
    }

    /// The balances of all coin types owned by this Owner.
    pub async fn balance_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Balance>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balances(self.address, first, after, last, before)
            .await
            .extend()
    }

    /// The coin objects for the given address or object.
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

    /// The `0x3::staking_pool::StakedSui` objects owned by the given object.
    pub async fn staked_sui_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, StakedSui>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_staked_sui(self.address, first, after, last, before)
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

    /// The SuinsRegistration NFTs owned by this address or object. These grant the owner the
    /// capability to manage the associated domain.
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

    /// Access a dynamic field on an object using its name.
    /// Names are arbitrary Move values whose type have `copy`, `drop`, and `store`, and are specified
    /// using their type, and their BCS contents, Base64 encoded.
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    pub async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicField)
            .await
            .extend()
    }

    /// Access a dynamic object field on an object using its name.
    /// Names are arbitrary Move values whose type have `copy`, `drop`, and `store`, and are specified
    /// using their type, and their BCS contents, Base64 encoded.
    /// The value of a dynamic object field can also be accessed off-chain directly via its address (e.g. using `Query.object`).
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    pub async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicObject)
            .await
            .extend()
    }

    /// The dynamic fields on an object.
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    pub async fn dynamic_field_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, DynamicField>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_fields(first, after, last, before, self.address)
            .await
            .extend()
    }
}
