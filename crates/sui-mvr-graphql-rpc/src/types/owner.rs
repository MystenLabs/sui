// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::address::Address;
use super::coin_metadata::CoinMetadata;
use super::cursor::Page;
use super::dynamic_field::DynamicField;
use super::dynamic_field::DynamicFieldName;
use super::move_package::MovePackage;
use super::stake::StakedSui;
use super::suins_registration::{DomainFormat, NameService, SuinsRegistration};
use crate::data::Db;
use crate::types::balance::{self, Balance};
use crate::types::coin::Coin;
use crate::types::move_object::MoveObject;
use crate::types::object::{self, Object, ObjectFilter};
use crate::types::sui_address::SuiAddress;
use crate::types::type_filter::ExactTypeFilter;

use async_graphql::connection::Connection;
use async_graphql::*;
use sui_name_service::NameServiceConfig;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::gas_coin::GAS;

#[derive(Clone, Debug)]
pub(crate) struct Owner {
    pub address: SuiAddress,
    /// The checkpoint sequence number at which this was viewed at.
    pub checkpoint_viewed_at: u64,
    /// Root parent object version for dynamic fields.
    ///
    /// This enables consistent dynamic field reads in the case of chained dynamic object fields,
    /// e.g., `Parent -> DOF1 -> DOF2`. In such cases, the object versions may end up like
    /// `Parent >= DOF1, DOF2` but `DOF1 < DOF2`. Thus, database queries for dynamic fields must
    /// bound the object versions by the version of the root object of the tree.
    ///
    /// Also, if this Owner is an object itself, `root_version` will be used to bound its version
    /// from above in [`Owner::as_object`].
    ///
    /// Essentially, lamport timestamps of objects are updated for all top-level mutable objects
    /// provided as inputs to a transaction as well as any mutated dynamic child objects. However,
    /// any dynamic child objects that were loaded but not actually mutated don't end up having
    /// their versions updated.
    pub root_version: Option<u64>,
}

/// Type to implement GraphQL fields that are shared by all Owners.
pub(crate) struct OwnerImpl {
    pub address: SuiAddress,
    /// The checkpoint sequence number at which this was viewed at.
    pub checkpoint_viewed_at: u64,
}

/// Interface implemented by GraphQL types representing entities that can own objects. Object owners
/// are identified by an address which can represent either the public key of an account or another
/// object. The same address can only refer to an account or an object, never both, but it is not
/// possible to know which up-front.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IOwner",
    field(name = "address", ty = "SuiAddress"),
    field(
        name = "objects",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        arg(name = "filter", ty = "Option<ObjectFilter>"),
        ty = "Connection<String, MoveObject>",
        desc = "Objects owned by this object or address, optionally `filter`-ed."
    ),
    field(
        name = "balance",
        arg(name = "type", ty = "Option<ExactTypeFilter>"),
        ty = "Option<Balance>",
        desc = "Total balance of all coins with marker type owned by this object or address. If \
                type is not supplied, it defaults to `0x2::sui::SUI`."
    ),
    field(
        name = "balances",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<balance::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<balance::Cursor>"),
        ty = "Connection<String, Balance>",
        desc = "The balances of all coin types owned by this object or address."
    ),
    field(
        name = "coins",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        arg(name = "type", ty = "Option<ExactTypeFilter>"),
        ty = "Connection<String, Coin>",
        desc = "The coin objects for this object or address.\n\n\
                `type` is a filter on the coin's type parameter, defaulting to `0x2::sui::SUI`."
    ),
    field(
        name = "staked_suis",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        ty = "Connection<String, StakedSui>",
        desc = "The `0x3::staking_pool::StakedSui` objects owned by this object or address."
    ),
    field(
        name = "default_suins_name",
        arg(name = "format", ty = "Option<DomainFormat>"),
        ty = "Option<String>",
        desc = "The domain explicitly configured as the default domain pointing to this object or \
                address."
    ),
    field(
        name = "suins_registrations",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        ty = "Connection<String, SuinsRegistration>",
        desc = "The SuinsRegistration NFTs owned by this object or address. These grant the owner \
                the capability to manage the associated domain."
    )
)]
pub(crate) enum IOwner {
    Owner(Owner),
    Address(Address),
    Object(Object),
    MovePackage(MovePackage),
    MoveObject(MoveObject),
    Coin(Coin),
    CoinMetadata(CoinMetadata),
    StakedSui(StakedSui),
    SuinsRegistration(SuinsRegistration),
}

/// An Owner is an entity that can own an object. Each Owner is identified by a SuiAddress which
/// represents either an Address (corresponding to a public key of an account) or an Object, but
/// never both (it is not known up-front whether a given Owner is an Address or an Object).
#[Object]
impl Owner {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(self).address().await
    }

    /// Objects owned by this object or address, optionally `filter`-ed.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(self)
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Total balance of all coins with marker type owned by this object or address. If type is not
    /// supplied, it defaults to `0x2::sui::SUI`.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        OwnerImpl::from(self).balance(ctx, type_).await
    }

    /// The balances of all coin types owned by this object or address.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        OwnerImpl::from(self)
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The coin objects for this object or address.
    ///
    ///`type` is a filter on the coin's type parameter, defaulting to `0x2::sui::SUI`.
    pub(crate) async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        OwnerImpl::from(self)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this object or address.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(self)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object or address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(self).default_suins_name(ctx, format).await
    }

    /// The SuinsRegistration NFTs owned by this object or address. These grant the owner the
    /// capability to manage the associated domain.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(self)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    async fn as_address(&self) -> Option<Address> {
        // For now only addresses can be owners
        Some(Address {
            address: self.address,
            checkpoint_viewed_at: self.checkpoint_viewed_at,
        })
    }

    async fn as_object(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        Object::query(
            ctx,
            self.address,
            if let Some(parent_version) = self.root_version {
                Object::under_parent(parent_version, self.checkpoint_viewed_at)
            } else {
                Object::latest_at(self.checkpoint_viewed_at)
            },
        )
        .await
        .extend()
    }

    /// Access a dynamic field on an object using its name. Names are arbitrary Move values whose
    /// type have `copy`, `drop`, and `store`, and are specified using their type, and their BCS
    /// contents, Base64 encoded.
    ///
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(self)
            .dynamic_field(ctx, name, self.root_version)
            .await
    }

    /// Access a dynamic object field on an object using its name. Names are arbitrary Move values
    /// whose type have `copy`, `drop`, and `store`, and are specified using their type, and their
    /// BCS contents, Base64 encoded. The value of a dynamic object field can also be accessed
    /// off-chain directly via its address (e.g. using `Query.object`).
    ///
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(self)
            .dynamic_object_field(ctx, name, self.root_version)
            .await
    }

    /// The dynamic fields and dynamic object fields on an object.
    ///
    /// This field exists as a convenience when accessing a dynamic field on a wrapped object.
    async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        OwnerImpl::from(self)
            .dynamic_fields(ctx, first, after, last, before, self.root_version)
            .await
    }
}

impl OwnerImpl {
    pub(crate) async fn address(&self) -> SuiAddress {
        self.address
    }

    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
            owner: Some(self.address),
            ..Default::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        MoveObject::paginate(
            ctx.data_unchecked(),
            page,
            filter,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        let coin = type_.map_or_else(GAS::type_tag, |t| t.0);
        Balance::query(
            ctx.data_unchecked(),
            self.address,
            coin,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Balance::paginate(
            ctx.data_unchecked(),
            page,
            self.address,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn coins(
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
        Coin::paginate(
            ctx.data_unchecked(),
            page,
            coin,
            Some(self.address),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        StakedSui::paginate(
            ctx.data_unchecked(),
            page,
            self.address,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        Ok(
            NameService::reverse_resolve_to_name(ctx, self.address, self.checkpoint_viewed_at)
                .await
                .extend()?
                .map(|d| d.format(format.unwrap_or(DomainFormat::Dot).into())),
        )
    }

    pub(crate) async fn suins_registrations(
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
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    // Dynamic field related functions are part of the `IMoveObject` interface, but are provided
    // here to implement convenience functions on `Owner` and `Object` to access dynamic fields.

    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
        parent_version: Option<u64>,
    ) -> Result<Option<DynamicField>> {
        use DynamicFieldType as T;
        DynamicField::query(
            ctx,
            self.address,
            parent_version,
            name,
            T::DynamicField,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
        parent_version: Option<u64>,
    ) -> Result<Option<DynamicField>> {
        use DynamicFieldType as T;
        DynamicField::query(
            ctx,
            self.address,
            parent_version,
            name,
            T::DynamicObject,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        parent_version: Option<u64>,
    ) -> Result<Connection<String, DynamicField>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        DynamicField::paginate(
            ctx.data_unchecked(),
            page,
            self.address,
            parent_version,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }
}

impl From<&Owner> for OwnerImpl {
    fn from(owner: &Owner) -> Self {
        OwnerImpl {
            address: owner.address,
            checkpoint_viewed_at: owner.checkpoint_viewed_at,
        }
    }
}
