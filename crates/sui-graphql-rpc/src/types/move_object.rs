// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::balance::{self, Balance};
use super::base64::Base64;
use super::big_int::BigInt;
use super::coin::CoinDowncastError;
use super::coin_metadata::{CoinMetadata, CoinMetadataDowncastError};
use super::cursor::Page;
use super::display::DisplayEntry;
use super::dynamic_field::{DynamicField, DynamicFieldName};
use super::move_type::MoveType;
use super::move_value::MoveValue;
use super::object::{self, ObjectFilter, ObjectImpl, ObjectLookup, ObjectOwner, ObjectStatus};
use super::owner::OwnerImpl;
use super::stake::StakedSuiDowncastError;
use super::sui_address::SuiAddress;
use super::suins_registration::{DomainFormat, SuinsRegistration, SuinsRegistrationDowncastError};
use super::transaction_block::{self, TransactionBlock, TransactionBlockFilter};
use super::type_filter::ExactTypeFilter;
use super::uint53::UInt53;
use super::{coin::Coin, object::Object};
use crate::connection::ScanConnection;
use crate::data::Db;
use crate::error::Error;
use crate::types::stake::StakedSui;
use async_graphql::connection::Connection;
use async_graphql::*;
use sui_name_service::NameServiceConfig;
use sui_types::object::{Data, MoveObject as NativeMoveObject};
use sui_types::TypeTag;

#[derive(Clone)]
pub(crate) struct MoveObject {
    /// Representation of this Move Object as a generic Object.
    pub super_: Object,

    /// Move-object-specific data, extracted from the native representation at
    /// `graphql_object.native_object.data`.
    pub native: NativeMoveObject,
}

/// Type to implement GraphQL fields that are shared by all MoveObjects.
pub(crate) struct MoveObjectImpl<'o>(pub &'o MoveObject);

pub(crate) enum MoveObjectDowncastError {
    WrappedOrDeleted,
    NotAMoveObject,
}

/// This interface is implemented by types that represent a Move object on-chain (A Move value whose
/// type has `key`).
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IMoveObject",
    field(
        name = "contents",
        ty = "Option<MoveValue>",
        desc = "Displays the contents of the Move object in a JSON string and through GraphQL \
                types. Also provides the flat representation of the type signature, and the BCS of \
                the corresponding data."
    ),
    field(
        name = "has_public_transfer",
        ty = "bool",
        desc = "Determines whether a transaction can transfer this object, using the \
                TransferObjects transaction command or `sui::transfer::public_transfer`, both of \
                which require the object to have the `key` and `store` abilities."
    ),
    field(
        name = "display",
        ty = "Option<Vec<DisplayEntry>>",
        desc = "The set of named templates defined on-chain for the type of this object, to be \
                handled off-chain. The server substitutes data from the object into these \
                templates to generate a display string per template."
    ),
    field(
        name = "dynamic_field",
        arg(name = "name", ty = "DynamicFieldName"),
        ty = "Option<DynamicField>",
        desc = "Access a dynamic field on an object using its name. Names are arbitrary Move \
                values whose type have `copy`, `drop`, and `store`, and are specified using their \
                type, and their BCS contents, Base64 encoded.\n\n\
                Dynamic fields on wrapped objects can be accessed by using the same API under the \
                Ownertype."
    ),
    field(
        name = "dynamic_object_field",
        arg(name = "name", ty = "DynamicFieldName"),
        ty = "Option<DynamicField>",
        desc = "Access a dynamic object field on an object using its name. Names are arbitrary \
                Move values whose type have `copy`, `drop`, and `store`, and are specified using \
                their type, and their BCS contents, Base64 encoded. The value of a dynamic object \
                field can also be accessed off-chain directly via its address (e.g. using \
                `Query.object`).\n\n\
                Dynamic fields on wrapped objects can be accessed by using the same API under the \
                Owner type."
    ),
    field(
        name = "dynamic_fields",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::Cursor>"),
        ty = "Connection<String, DynamicField>",
        desc = "The dynamic fields and dynamic object fields on an object.\n\n\
                Dynamic fields on wrapped objects can be accessed by using the same API under the \
                Owner type."
    )
)]
pub(crate) enum IMoveObject {
    MoveObject(MoveObject),
    Coin(Coin),
    CoinMetadata(CoinMetadata),
    StakedSui(StakedSui),
    SuinsRegistration(SuinsRegistration),
}

/// The representation of an object as a Move Object, which exposes additional information
/// (content, module that governs it, version, is transferrable, etc.) about this object.
#[Object]
impl MoveObject {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(&self.super_).address().await
    }

    /// Objects owned by this object, optionally `filter`-ed.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(&self.super_)
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Total balance of all coins with marker type owned by this object. If type is not supplied,
    /// it defaults to `0x2::sui::SUI`.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        OwnerImpl::from(&self.super_).balance(ctx, type_).await
    }

    /// The balances of all coin types owned by this object.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        OwnerImpl::from(&self.super_)
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The coin objects for this object.
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
        OwnerImpl::from(&self.super_)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this object.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(&self.super_)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(&self.super_)
            .default_suins_name(ctx, format)
            .await
    }

    /// The SuinsRegistration NFTs owned by this object. These grant the owner the capability to
    /// manage the associated domain.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(&self.super_)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl(&self.super_).version().await
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// NOT_INDEXED, the object is loaded from serialized data, such as the contents of a genesis or
    /// system package upgrade transaction. LIVE, the version returned is the most recent for the
    /// object, and it is not deleted or wrapped at that version. HISTORICAL, the object was
    /// referenced at a specific version or checkpoint, so is fetched from historical tables and may
    /// not be the latest version of the object. WRAPPED_OR_DELETED, the object is deleted or
    /// wrapped and only partial information can be loaded."
    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectImpl(&self.super_).status().await
    }

    /// 32-byte hash that identifies the object's contents, encoded as a Base58 string.
    pub(crate) async fn digest(&self) -> Option<String> {
        ObjectImpl(&self.super_).digest().await
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    pub(crate) async fn owner(&self) -> Option<ObjectOwner> {
        ObjectImpl(&self.super_).owner().await
    }

    /// The transaction block that created this version of the object.
    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        ObjectImpl(&self.super_)
            .previous_transaction_block(ctx)
            .await
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated. This number is
    /// recalculated based on the present storage gas price.
    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        ObjectImpl(&self.super_).storage_rebate().await
    }

    /// The transaction blocks that sent objects to this object.
    ///
    /// `scanLimit` restricts the number of candidate transactions scanned when gathering a page of
    /// results. It is required for queries that apply more than two complex filters (on function,
    /// kind, sender, recipient, input object, changed object, or ids), and can be at most
    /// `serviceConfig.maxScanLimit`.
    ///
    /// When the scan limit is reached the page will be returned even if it has fewer than `first`
    /// results when paginating forward (`last` when paginating backwards). If there are more
    /// transactions to scan, `pageInfo.hasNextPage` (or `pageInfo.hasPreviousPage`) will be set to
    /// `true`, and `PageInfo.endCursor` (or `PageInfo.startCursor`) will be set to the last
    /// transaction that was scanned as opposed to the last (or first) transaction in the page.
    ///
    /// Requesting the next (or previous) page after this cursor will resume the search, scanning
    /// the next `scanLimit` many transactions in the direction of pagination, and so on until all
    /// transactions in the scanning range have been visited.
    ///
    /// By default, the scanning range includes all transactions known to GraphQL, but it can be
    /// restricted by the `after` and `before` cursors, and the `beforeCheckpoint`,
    /// `afterCheckpoint` and `atCheckpoint` filters.
    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>> {
        ObjectImpl(&self.super_)
            .received_transaction_blocks(ctx, first, after, last, before, filter, scan_limit)
            .await
    }

    /// The Base64-encoded BCS serialization of the object's content.
    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        ObjectImpl(&self.super_).bcs().await
    }

    /// Displays the contents of the Move object in a JSON string and through GraphQL types. Also
    /// provides the flat representation of the type signature, and the BCS of the corresponding
    /// data.
    pub(crate) async fn contents(&self) -> Option<MoveValue> {
        MoveObjectImpl(self).contents().await
    }

    /// Determines whether a transaction can transfer this object, using the TransferObjects
    /// transaction command or `sui::transfer::public_transfer`, both of which require the object to
    /// have the `key` and `store` abilities.
    pub(crate) async fn has_public_transfer(&self, ctx: &Context<'_>) -> Result<bool> {
        MoveObjectImpl(self).has_public_transfer(ctx).await
    }

    /// The set of named templates defined on-chain for the type of this object, to be handled
    /// off-chain. The server substitutes data from the object into these templates to generate a
    /// display string per template.
    pub(crate) async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        ObjectImpl(&self.super_).display(ctx).await
    }

    /// Access a dynamic field on an object using its name. Names are arbitrary Move values whose
    /// type have `copy`, `drop`, and `store`, and are specified using their type, and their BCS
    /// contents, Base64 encoded.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(&self.super_)
            .dynamic_field(ctx, name, Some(self.root_version()))
            .await
    }

    /// Access a dynamic object field on an object using its name. Names are arbitrary Move values
    /// whose type have `copy`, `drop`, and `store`, and are specified using their type, and their
    /// BCS contents, Base64 encoded. The value of a dynamic object field can also be accessed
    /// off-chain directly via its address (e.g. using `Query.object`).
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(&self.super_)
            .dynamic_object_field(ctx, name, Some(self.root_version()))
            .await
    }

    /// The dynamic fields and dynamic object fields on an object.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        OwnerImpl::from(&self.super_)
            .dynamic_fields(ctx, first, after, last, before, Some(self.root_version()))
            .await
    }

    /// Attempts to convert the Move object into a `0x2::coin::Coin`.
    async fn as_coin(&self) -> Result<Option<Coin>> {
        match Coin::try_from(self) {
            Ok(coin) => Ok(Some(coin)),
            Err(CoinDowncastError::NotACoin) => Ok(None),
            Err(CoinDowncastError::Bcs(e)) => {
                Err(Error::Internal(format!("Failed to deserialize Coin: {e}"))).extend()
            }
        }
    }

    /// Attempts to convert the Move object into a `0x3::staking_pool::StakedSui`.
    async fn as_staked_sui(&self) -> Result<Option<StakedSui>> {
        match StakedSui::try_from(self) {
            Ok(coin) => Ok(Some(coin)),
            Err(StakedSuiDowncastError::NotAStakedSui) => Ok(None),
            Err(StakedSuiDowncastError::Bcs(e)) => Err(Error::Internal(format!(
                "Failed to deserialize StakedSui: {e}"
            )))
            .extend(),
        }
    }

    /// Attempts to convert the Move object into a `0x2::coin::CoinMetadata`.
    async fn as_coin_metadata(&self) -> Result<Option<CoinMetadata>> {
        match CoinMetadata::try_from(self) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(CoinMetadataDowncastError::NotCoinMetadata) => Ok(None),
            Err(CoinMetadataDowncastError::Bcs(e)) => Err(Error::Internal(format!(
                "Failed to deserialize CoinMetadata: {e}"
            )))
            .extend(),
        }
    }

    /// Attempts to convert the Move object into a `SuinsRegistration` object.
    async fn as_suins_registration(&self, ctx: &Context<'_>) -> Result<Option<SuinsRegistration>> {
        let cfg: &NameServiceConfig = ctx.data_unchecked();
        let tag = SuinsRegistration::type_(cfg.package_address.into());

        match SuinsRegistration::try_from(self, &tag) {
            Ok(registration) => Ok(Some(registration)),
            Err(SuinsRegistrationDowncastError::NotASuinsRegistration) => Ok(None),
            Err(SuinsRegistrationDowncastError::Bcs(e)) => Err(Error::Internal(format!(
                "Failed to deserialize SuinsRegistration: {e}",
            )))
            .extend(),
        }
    }
}

impl MoveObjectImpl<'_> {
    pub(crate) async fn contents(&self) -> Option<MoveValue> {
        let type_ = TypeTag::from(self.0.native.type_().clone());
        Some(MoveValue::new(type_, self.0.native.contents().into()))
    }

    pub(crate) async fn has_public_transfer(&self, ctx: &Context<'_>) -> Result<bool> {
        let type_: MoveType = self.0.native.type_().clone().into();
        let set = type_.abilities_impl(ctx.data_unchecked()).await.extend()?;
        Ok(set.is_some_and(|s| s.has_key() && s.has_store()))
    }
}

impl MoveObject {
    pub(crate) async fn query(
        ctx: &Context<'_>,
        address: SuiAddress,
        key: ObjectLookup,
    ) -> Result<Option<Self>, Error> {
        let Some(object) = Object::query(ctx, address, key).await? else {
            return Ok(None);
        };

        match MoveObject::try_from(&object) {
            Ok(object) => Ok(Some(object)),
            Err(MoveObjectDowncastError::WrappedOrDeleted) => Ok(None),
            Err(MoveObjectDowncastError::NotAMoveObject) => {
                Err(Error::Internal(format!("{address} is not a Move object")))?
            }
        }
    }

    /// Query the database for a `page` of Move objects, optionally `filter`-ed.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this page was
    /// queried for. Each entity returned in the connection will inherit this checkpoint, so that
    /// when viewing that entity's state, it will be as if it was read at the same checkpoint.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<object::Cursor>,
        filter: ObjectFilter,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, MoveObject>, Error> {
        Object::paginate_subtype(db, page, filter, checkpoint_viewed_at, |object| {
            let address = object.address;
            MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {address} to be a Move object, but it's not."
                ))
            })
        })
        .await
    }

    /// Root parent object version for dynamic fields.
    ///
    /// Check [`Object::root_version`] for details.
    pub(crate) fn root_version(&self) -> u64 {
        self.super_.root_version()
    }
}

impl TryFrom<&Object> for MoveObject {
    type Error = MoveObjectDowncastError;

    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        let Some(native) = object.native_impl() else {
            return Err(MoveObjectDowncastError::WrappedOrDeleted);
        };

        if let Data::Move(move_object) = &native.data {
            Ok(Self {
                super_: object.clone(),
                native: move_object.clone(),
            })
        } else {
            Err(MoveObjectDowncastError::NotAMoveObject)
        }
    }
}
