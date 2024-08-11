// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use super::available_range::AvailableRange;
use super::balance::{self, Balance};
use super::big_int::BigInt;
use super::coin::Coin;
use super::coin_metadata::CoinMetadata;
use super::cursor::{self, Page, RawPaginated, Target};
use super::digest::Digest;
use super::display::{Display, DisplayEntry};
use super::dynamic_field::{DynamicField, DynamicFieldName};
use super::move_object::MoveObject;
use super::move_package::MovePackage;
use super::owner::OwnerImpl;
use super::stake::StakedSui;
use super::suins_registration::{DomainFormat, SuinsRegistration};
use super::transaction_block;
use super::transaction_block::TransactionBlockFilter;
use super::type_filter::{ExactTypeFilter, TypeFilter};
use super::uint53::UInt53;
use super::{owner::Owner, sui_address::SuiAddress, transaction_block::TransactionBlock};
use crate::consistency::{build_objects_query, Checkpointed, View};
use crate::data::package_resolver::PackageResolver;
use crate::data::{DataLoader, Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::raw_query::RawQuery;
use crate::types::base64::Base64;
use crate::types::intersect;
use crate::{filter, or_filter};
use async_graphql::connection::{CursorType, Edge};
use async_graphql::dataloader::Loader;
use async_graphql::{connection::Connection, *};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_indexer::models::objects::{StoredDeletedHistoryObject, StoredHistoryObject};
use sui_indexer::schema::objects_history;
use sui_indexer::types::ObjectStatus as NativeObjectStatus;
use sui_indexer::types::OwnerType;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::object::{
    MoveObject as NativeMoveObject, Object as NativeObject, Owner as NativeOwner,
};
use sui_types::TypeTag;

#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,
    pub kind: ObjectKind,
    /// The checkpoint sequence number at which this was viewed at.
    pub checkpoint_viewed_at: u64,
    /// Root parent object version for dynamic fields.
    ///
    /// This enables consistent dynamic field reads in the case of chained dynamic object fields,
    /// e.g., `Parent -> DOF1 -> DOF2`. In such cases, the object versions may end up like
    /// `Parent >= DOF1, DOF2` but `DOF1 < DOF2`. Thus, database queries for dynamic fields must
    /// bound the object versions by the version of the root object of the tree.
    ///
    /// Essentially, lamport timestamps of objects are updated for all top-level mutable objects
    /// provided as inputs to a transaction as well as any mutated dynamic child objects. However,
    /// any dynamic child objects that were loaded but not actually mutated don't end up having
    /// their versions updated.
    root_version: u64,
}

/// Type to implement GraphQL fields that are shared by all Objects.
pub(crate) struct ObjectImpl<'o>(pub &'o Object);

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ObjectKind {
    /// An object loaded from serialized data, such as the contents of a transaction that hasn't
    /// been indexed yet.
    NotIndexed(NativeObject),
    /// An object fetched from the index.
    Indexed(NativeObject, StoredHistoryObject),
    /// The object is wrapped or deleted and only partial information can be loaded from the
    /// indexer.
    WrappedOrDeleted(StoredDeletedHistoryObject),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[graphql(name = "ObjectKind")]
pub enum ObjectStatus {
    /// The object is loaded from serialized data, such as the contents of a transaction that hasn't
    /// been indexed yet.
    NotIndexed,
    /// The object is fetched from the index.
    Indexed,
    /// The object is deleted or wrapped and only partial information can be loaded from the
    /// indexer.
    WrappedOrDeleted,
}

#[derive(Clone, Debug, PartialEq, Eq, InputObject)]
pub(crate) struct ObjectRef {
    /// ID of the object.
    pub address: SuiAddress,
    /// Version or sequence number of the object.
    pub version: UInt53,
    /// Digest of the object.
    pub digest: Digest,
}

/// Constrains the set of objects returned. All filters are optional, and the resulting set of
/// objects are ones whose
///
/// - Type matches the `type` filter,
/// - AND, whose owner matches the `owner` filter,
/// - AND, whose ID is in `objectIds` OR whose ID and version is in `objectKeys`.
#[derive(InputObject, Default, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectFilter {
    /// This field is used to specify the type of objects that should be included in the query
    /// results.
    ///
    /// Objects can be filtered by their type's package, package::module, or their fully qualified
    /// type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g. `0x2::coin::Coin`, or by
    /// the full type name, such as `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<TypeFilter>,

    /// Filter for live objects by their current owners.
    pub owner: Option<SuiAddress>,

    /// Filter for live objects by their IDs.
    pub object_ids: Option<Vec<SuiAddress>>,

    /// Filter for live or potentially historical objects by their ID and version.
    pub object_keys: Option<Vec<ObjectKey>>,
}

#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    pub object_id: SuiAddress,
    pub version: UInt53,
}

/// The object's owner type: Immutable, Shared, Parent, or Address.
#[derive(Union, Clone)]
pub(crate) enum ObjectOwner {
    Immutable(Immutable),
    Shared(Shared),
    Parent(Parent),
    Address(AddressOwner),
}

/// An immutable object is an object that can't be mutated, transferred, or deleted.
/// Immutable objects have no owner, so anyone can use them.
#[derive(SimpleObject, Clone)]
pub(crate) struct Immutable {
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// A shared object is an object that is shared using the 0x2::transfer::share_object function.
/// Unlike owned objects, once an object is shared, it stays mutable and is accessible by anyone.
#[derive(SimpleObject, Clone)]
pub(crate) struct Shared {
    initial_shared_version: UInt53,
}

/// If the object's owner is a Parent, this object is part of a dynamic field (it is the value of
/// the dynamic field, or the intermediate Field object itself). Also note that if the owner
/// is a parent, then it's guaranteed to be an object.
#[derive(SimpleObject, Clone)]
pub(crate) struct Parent {
    parent: Option<Object>,
}

/// An address-owned object is owned by a specific 32-byte address that is
/// either an account address (derived from a particular signature scheme) or
/// an object ID. An address-owned object is accessible only to its owner and no others.
#[derive(SimpleObject, Clone)]
pub(crate) struct AddressOwner {
    owner: Option<Owner>,
}

pub(crate) enum ObjectLookup {
    LatestAt {
        /// The parent version to be used as an optional upper bound for the query. Look for the
        /// latest version of a child object that is less than or equal to this upper bound.
        parent_version: Option<u64>,
        /// The checkpoint sequence number at which this was viewed at
        checkpoint_viewed_at: u64,
    },

    VersionAt {
        /// The exact version of the object to be fetched.
        version: u64,
        /// The checkpoint sequence number at which this was viewed at.
        checkpoint_viewed_at: u64,
    },
}

pub(crate) type Cursor = cursor::BcsCursor<HistoricalObjectCursor>;

/// The inner struct for the `Object`'s cursor. The `object_id` is used as the cursor, while the
/// `checkpoint_viewed_at` sets the consistent upper bound for the cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct HistoricalObjectCursor {
    #[serde(rename = "o")]
    object_id: Vec<u8>,
    /// The checkpoint sequence number this was viewed at.
    #[serde(rename = "c")]
    checkpoint_viewed_at: u64,
}

/// Interface implemented by on-chain values that are addressable by an ID (also referred to as its
/// address). This includes Move objects and packages.
#[derive(Interface)]
#[graphql(
    name = "IObject",
    field(name = "version", ty = "UInt53"),
    field(
        name = "status",
        ty = "ObjectStatus",
        desc = "The current status of the object as read from the off-chain store. The possible \
                states are: NOT_INDEXED, the object is loaded from serialized data, such as the \
                contents of a genesis or system package upgrade transaction. LIVE, the version \
                returned is the most recent for the object, and it is not deleted or wrapped at \
                that version. HISTORICAL, the object was referenced at a specific version or \
                checkpoint, so is fetched from historical tables and may not be the latest version \
                of the object. WRAPPED_OR_DELETED, the object is deleted or wrapped and only \
                partial information can be loaded."
    ),
    field(
        name = "digest",
        ty = "Option<String>",
        desc = "32-byte hash that identifies the object's current contents, encoded as a Base58 \
                string."
    ),
    field(
        name = "owner",
        ty = "Option<ObjectOwner>",
        desc = "The owner type of this object: Immutable, Shared, Parent, Address\n\
                Immutable and Shared Objects do not have owners."
    ),
    field(
        name = "previous_transaction_block",
        ty = "Option<TransactionBlock>",
        desc = "The transaction block that created this version of the object."
    ),
    field(name = "storage_rebate", ty = "Option<BigInt>", desc = "",),
    field(
        name = "received_transaction_blocks",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<transaction_block::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<transaction_block::Cursor>"),
        arg(name = "filter", ty = "Option<TransactionBlockFilter>"),
        ty = "Connection<String, TransactionBlock>",
        desc = "The transaction blocks that sent objects to this object."
    ),
    field(
        name = "bcs",
        ty = "Option<Base64>",
        desc = "The Base64-encoded BCS serialization of the object's content."
    )
)]
pub(crate) enum IObject {
    Object(Object),
    MovePackage(MovePackage),
    MoveObject(MoveObject),
    Coin(Coin),
    CoinMetadata(CoinMetadata),
    StakedSui(StakedSui),
    SuinsRegistration(SuinsRegistration),
}

/// DataLoader key for fetching an `Object` at a specific version, constrained by a consistency
/// cursor (if that version was created after the checkpoint the query is viewing at, then it will
/// fail).
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct HistoricalKey {
    id: SuiAddress,
    version: u64,
    checkpoint_viewed_at: u64,
}

/// DataLoader key for fetching the latest version of an `Object` as of a consistency cursor. The
/// query can optionally be bounded by a `parent_version` which imposes an additional requirement
/// that the object's version is bounded above by the parent version.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct LatestAtKey {
    id: SuiAddress,
    parent_version: Option<u64>,
    checkpoint_viewed_at: u64,
}

/// An object in Sui is a package (set of Move bytecode modules) or object (typed data structure
/// with fields) with additional metadata detailing its id, version, transaction digest, owner
/// field indicating how this object can be accessed.
#[Object]
impl Object {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(self).address().await
    }

    /// Objects owned by this object, optionally `filter`-ed.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(self)
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
        OwnerImpl::from(self).balance(ctx, type_).await
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
        OwnerImpl::from(self)
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
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        OwnerImpl::from(self)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this object.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(self)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(self).default_suins_name(ctx, format).await
    }

    /// The SuinsRegistration NFTs owned by this object. These grant the owner the capability to
    /// manage the associated domain.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(self)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl(self).version().await
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// NOT_INDEXED, the object is loaded from serialized data, such as the contents of a genesis or
    /// system package upgrade transaction. LIVE, the version returned is the most recent for the
    /// object, and it is not deleted or wrapped at that version. HISTORICAL, the object was
    /// referenced at a specific version or checkpoint, so is fetched from historical tables and may
    /// not be the latest version of the object. WRAPPED_OR_DELETED, the object is deleted or
    /// wrapped and only partial information can be loaded."
    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectImpl(self).status().await
    }

    /// 32-byte hash that identifies the object's current contents, encoded as a Base58 string.
    pub(crate) async fn digest(&self) -> Option<String> {
        ObjectImpl(self).digest().await
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    /// Immutable and Shared Objects do not have owners.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Option<ObjectOwner> {
        ObjectImpl(self).owner(ctx).await
    }

    /// The transaction block that created this version of the object.
    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        ObjectImpl(self).previous_transaction_block(ctx).await
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated. This number is
    /// recalculated based on the present storage gas price.
    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        ObjectImpl(self).storage_rebate().await
    }

    /// The transaction blocks that sent objects to this object.
    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        ObjectImpl(self)
            .received_transaction_blocks(ctx, first, after, last, before, filter)
            .await
    }

    /// The Base64-encoded BCS serialization of the object's content.
    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        ObjectImpl(self).bcs().await
    }

    /// The set of named templates defined on-chain for the type of this object, to be handled
    /// off-chain. The server substitutes data from the object into these templates to generate a
    /// display string per template.
    async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        ObjectImpl(self).display(ctx).await
    }

    /// Access a dynamic field on an object using its name. Names are arbitrary Move values whose
    /// type have `copy`, `drop`, and `store`, and are specified using their type, and their BCS
    /// contents, Base64 encoded.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(self)
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
    async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(self)
            .dynamic_object_field(ctx, name, Some(self.root_version()))
            .await
    }

    /// The dynamic fields and dynamic object fields on an object.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        OwnerImpl::from(self)
            .dynamic_fields(ctx, first, after, last, before, Some(self.root_version()))
            .await
    }

    /// Attempts to convert the object into a MoveObject
    async fn as_move_object(&self) -> Option<MoveObject> {
        MoveObject::try_from(self).ok()
    }

    /// Attempts to convert the object into a MovePackage
    async fn as_move_package(&self) -> Option<MovePackage> {
        MovePackage::try_from(self).ok()
    }
}

impl ObjectImpl<'_> {
    pub(crate) async fn version(&self) -> UInt53 {
        self.0.version_impl().into()
    }

    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectStatus::from(&self.0.kind)
    }

    pub(crate) async fn digest(&self) -> Option<String> {
        self.0
            .native_impl()
            .map(|native| native.digest().base58_encode())
    }

    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Option<ObjectOwner> {
        use NativeOwner as O;

        let Some(native) = self.0.native_impl() else {
            return None;
        };

        match native.owner {
            O::AddressOwner(address) => {
                let address = SuiAddress::from(address);
                Some(ObjectOwner::Address(AddressOwner {
                    owner: Some(Owner {
                        address,
                        checkpoint_viewed_at: self.0.checkpoint_viewed_at,
                        root_version: None,
                    }),
                }))
            }
            O::Immutable => Some(ObjectOwner::Immutable(Immutable { dummy: None })),
            O::ObjectOwner(address) => {
                let parent = Object::query(
                    ctx,
                    address.into(),
                    Object::latest_at(self.0.checkpoint_viewed_at),
                )
                .await
                .ok()
                .flatten();

                Some(ObjectOwner::Parent(Parent { parent }))
            }
            O::Shared {
                initial_shared_version,
            } => Some(ObjectOwner::Shared(Shared {
                initial_shared_version: initial_shared_version.value().into(),
            })),
        }
    }

    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        let Some(native) = self.0.native_impl() else {
            return Ok(None);
        };
        let digest = native.previous_transaction;

        TransactionBlock::query(ctx, digest.into(), self.0.checkpoint_viewed_at)
            .await
            .extend()
    }

    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        self.0
            .native_impl()
            .map(|native| BigInt::from(native.storage_rebate))
    }

    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter
            .unwrap_or_default()
            .intersect(TransactionBlockFilter {
                recv_address: Some(self.0.address),
                ..Default::default()
            })
        else {
            return Ok(Connection::new(false, false));
        };

        TransactionBlock::paginate(
            ctx.data_unchecked(),
            page,
            filter,
            self.0.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        use ObjectKind as K;
        Ok(match &self.0.kind {
            K::WrappedOrDeleted(_) => None,
            // WrappedOrDeleted objects are also read from the historical objects table, and they do
            // not have a serialized object, so the column is also nullable for stored historical
            // objects.
            K::Indexed(_, stored) => stored.serialized_object.as_ref().map(Base64::from),

            K::NotIndexed(native) => {
                let bytes = bcs::to_bytes(native)
                    .map_err(|e| {
                        Error::Internal(format!(
                            "Failed to serialize object at {}: {e}",
                            self.0.address
                        ))
                    })
                    .extend()?;
                Some(Base64::from(&bytes))
            }
        })
    }

    /// `display` is part of the `IMoveObject` interface, but is implemented on `ObjectImpl` to
    /// allow for a convenience function on `Object`.
    pub(crate) async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        let Some(native) = self.0.native_impl() else {
            return Ok(None);
        };

        let move_object = native
            .data
            .try_as_move()
            .ok_or_else(|| Error::Internal("Failed to convert object into MoveObject".to_string()))
            .extend()?;

        let (struct_tag, move_struct) = deserialize_move_struct(move_object, ctx.data_unchecked())
            .await
            .extend()?;

        let Some(display) = Display::query(ctx.data_unchecked(), struct_tag.into())
            .await
            .extend()?
        else {
            return Ok(None);
        };

        Ok(Some(display.render(&move_struct).extend()?))
    }
}

impl Object {
    /// Construct a GraphQL object from a native object, without its stored (indexed) counterpart.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this `Object` was
    /// constructed in. This is stored on `Object` so that when viewing that entity's state, it will
    /// be as if it was read at the same checkpoint.
    ///
    /// `root_version` represents the version of the root object in some nested chain of dynamic
    /// fields. This should typically be left `None`, unless the object(s) being resolved is a
    /// dynamic field, or if `root_version` has been explicitly set for this object. If None, then
    /// we use [`version_for_dynamic_fields`] to infer a root version to then propagate from this
    /// object down to its dynamic fields.
    pub(crate) fn from_native(
        address: SuiAddress,
        native: NativeObject,
        checkpoint_viewed_at: u64,
        root_version: Option<u64>,
    ) -> Object {
        let root_version = root_version.unwrap_or_else(|| version_for_dynamic_fields(&native));
        Object {
            address,
            kind: ObjectKind::NotIndexed(native),
            checkpoint_viewed_at,
            root_version,
        }
    }

    pub(crate) fn native_impl(&self) -> Option<&NativeObject> {
        use ObjectKind as K;

        match &self.kind {
            K::NotIndexed(native) | K::Indexed(native, _) => Some(native),
            K::WrappedOrDeleted(_) => None,
        }
    }

    pub(crate) fn version_impl(&self) -> u64 {
        use ObjectKind as K;

        match &self.kind {
            K::NotIndexed(native) | K::Indexed(native, _) => native.version().value(),
            K::WrappedOrDeleted(stored) => stored.object_version as u64,
        }
    }

    /// Root parent object version for dynamic fields.
    ///
    /// Check [`Object::root_version`] for details.
    pub(crate) fn root_version(&self) -> u64 {
        self.root_version
    }

    /// Query the database for a `page` of objects, optionally `filter`-ed.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this page was
    /// queried for. Each entity returned in the connection will inherit this checkpoint, so that
    /// when viewing that entity's state, it will be as if it was read at the same checkpoint.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        filter: ObjectFilter,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, Object>, Error> {
        Self::paginate_subtype(db, page, filter, checkpoint_viewed_at, Ok).await
    }

    /// Query the database for a `page` of some sub-type of Object. The page uses the bytes of an
    /// Object ID and the checkpoint when the query was made as the cursor, and can optionally be
    /// further `filter`-ed. The subtype is created using the `downcast` function, which is allowed
    /// to fail, if the downcast has failed.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this page was
    /// queried for. Each entity returned in the connection will inherit this checkpoint, so that
    /// when viewing that entity's state, it will be as if it was read at the same checkpoint.
    ///
    /// If a `Page<Cursor>` is also provided, then this function will defer to the
    /// `checkpoint_viewed_at` in the cursors. Otherwise, use the value from the parameter, or set
    /// to None. This is so that paginated queries are consistent with the previous query that
    /// created the cursor.
    pub(crate) async fn paginate_subtype<T: OutputType>(
        db: &Db,
        page: Page<Cursor>,
        filter: ObjectFilter,
        checkpoint_viewed_at: u64,
        downcast: impl Fn(Object) -> Result<T, Error>,
    ) -> Result<Connection<String, T>, Error> {
        // If cursors are provided, defer to the `checkpoint_viewed_at` in the cursor if they are
        // consistent. Otherwise, use the value from the parameter, or set to None. This is so that
        // paginated queries are consistent with the previous query that created the cursor.
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);

        let Some((prev, next, results)) = db
            .execute_repeatable(move |conn| {
                let Some(range) = AvailableRange::result(conn, checkpoint_viewed_at)? else {
                    return Ok::<_, diesel::result::Error>(None);
                };

                Ok(Some(page.paginate_raw_query::<StoredHistoryObject>(
                    conn,
                    checkpoint_viewed_at,
                    objects_query(&filter, range, &page),
                )?))
            })
            .await?
        else {
            return Err(Error::Client(
                "Requested data is outside the available range".to_string(),
            ));
        };

        let mut conn: Connection<String, T> = Connection::new(prev, next);
        for stored in results {
            // To maintain consistency, the returned cursor should have the same upper-bound as the
            // checkpoint found on the cursor.
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            let object =
                Object::try_from_stored_history_object(stored, checkpoint_viewed_at, None)?;
            conn.edges.push(Edge::new(cursor, downcast(object)?));
        }

        Ok(conn)
    }

    /// Look-up the latest version of the object as of a given checkpoint.
    pub(crate) fn latest_at(checkpoint_viewed_at: u64) -> ObjectLookup {
        ObjectLookup::LatestAt {
            parent_version: None,
            checkpoint_viewed_at,
        }
    }

    /// Look-up the latest version of an object whose version is less than or equal to its parent's
    /// version, as of a given checkpoint.
    pub(crate) fn under_parent(parent_version: u64, checkpoint_viewed_at: u64) -> ObjectLookup {
        ObjectLookup::LatestAt {
            parent_version: Some(parent_version),
            checkpoint_viewed_at,
        }
    }

    /// Look-up a specific version of the object, as of a given checkpoint.
    pub(crate) fn at_version(version: u64, checkpoint_viewed_at: u64) -> ObjectLookup {
        ObjectLookup::VersionAt {
            version,
            checkpoint_viewed_at,
        }
    }

    pub(crate) async fn query(
        ctx: &Context<'_>,
        id: SuiAddress,
        key: ObjectLookup,
    ) -> Result<Option<Self>, Error> {
        let DataLoader(loader) = &ctx.data_unchecked();

        match key {
            ObjectLookup::VersionAt {
                version,
                checkpoint_viewed_at,
            } => {
                loader
                    .load_one(HistoricalKey {
                        id,
                        version,
                        checkpoint_viewed_at,
                    })
                    .await
            }
            ObjectLookup::LatestAt {
                parent_version,
                checkpoint_viewed_at,
            } => {
                loader
                    .load_one(LatestAtKey {
                        id,
                        parent_version,
                        checkpoint_viewed_at,
                    })
                    .await
            }
        }
    }

    /// Query for a singleton object identified by its type. Note: the object is assumed to be a
    /// singleton (we either find at least one object with this type and then return it, or return
    /// nothing).
    pub(crate) async fn query_singleton(
        db: &Db,
        type_: StructTag,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<Object>, Error> {
        let filter = ObjectFilter {
            type_: Some(TypeFilter::ByType(type_)),
            ..Default::default()
        };

        let connection = Self::paginate(db, Page::bounded(1), filter, checkpoint_viewed_at).await?;

        Ok(connection.edges.into_iter().next().map(|edge| edge.node))
    }

    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this `Object` was
    /// constructed in. This is stored on `Object` so that when viewing that entity's state, it will
    /// be as if it was read at the same checkpoint.
    ///
    /// `root_version` represents the version of the root object in some nested chain of dynamic
    /// fields. This should typically be left `None`, unless the object(s) being resolved is a
    /// dynamic field, or if `root_version` has been explicitly set for this object. If None, then
    /// we use [`version_for_dynamic_fields`] to infer a root version to then propagate from this
    /// object down to its dynamic fields.
    pub(crate) fn try_from_stored_history_object(
        history_object: StoredHistoryObject,
        checkpoint_viewed_at: u64,
        root_version: Option<u64>,
    ) -> Result<Self, Error> {
        let address = addr(&history_object.object_id)?;

        let object_status =
            NativeObjectStatus::try_from(history_object.object_status).map_err(|_| {
                Error::Internal(format!(
                    "Unknown object status {} for object {} at version {}",
                    history_object.object_status, address, history_object.object_version
                ))
            })?;

        match object_status {
            NativeObjectStatus::Active => {
                let Some(serialized_object) = &history_object.serialized_object else {
                    return Err(Error::Internal(format!(
                        "Live object {} at version {} cannot have missing serialized_object field",
                        address, history_object.object_version
                    )));
                };

                let native_object = bcs::from_bytes(serialized_object).map_err(|_| {
                    Error::Internal(format!("Failed to deserialize object {address}"))
                })?;

                let root_version =
                    root_version.unwrap_or_else(|| version_for_dynamic_fields(&native_object));
                Ok(Self {
                    address,
                    kind: ObjectKind::Indexed(native_object, history_object),
                    checkpoint_viewed_at,
                    root_version,
                })
            }
            NativeObjectStatus::WrappedOrDeleted => Ok(Self {
                address,
                kind: ObjectKind::WrappedOrDeleted(StoredDeletedHistoryObject {
                    object_id: history_object.object_id,
                    object_version: history_object.object_version,
                    object_status: history_object.object_status,
                    checkpoint_sequence_number: history_object.checkpoint_sequence_number,
                }),
                checkpoint_viewed_at,
                root_version: history_object.object_version as u64,
            }),
        }
    }
}

/// We're deliberately choosing to use a child object's version as the root here, and letting the
/// caller override it with the actual root object's version if it has access to it.
///
/// Using the child object's version as the root means that we're seeing the dynamic field tree
/// under this object at the state resulting from the transaction that produced this version.
///
/// See [`Object::root_version`] for more details on parent/child object version mechanics.
fn version_for_dynamic_fields(native: &NativeObject) -> u64 {
    native.as_inner().version().into()
}

impl ObjectFilter {
    /// Try to create a filter whose results are the intersection of objects in `self`'s results and
    /// objects in `other`'s results. This may not be possible if the resulting filter is
    /// inconsistent in some way (e.g. a filter that requires one field to be two different values
    /// simultaneously).
    pub(crate) fn intersect(self, other: ObjectFilter) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        // Treat `object_ids` and `object_keys` as a single filter on IDs, and optionally versions,
        // and compute the intersection of that.
        let keys = intersect::field(self.keys(), other.keys(), |k, l| {
            let mut combined = BTreeMap::new();

            for (id, v) in k {
                if let Some(w) = l.get(&id).copied() {
                    combined.insert(id, intersect::field(v, w, intersect::by_eq)?);
                }
            }

            // If the intersection is empty, it means, there were some ID or Key filters in both
            // `self` and `other`, but they don't overlap, so the final result is inconsistent.
            (!combined.is_empty()).then_some(combined)
        })?;

        // Extract the ID and Key filters back out. At this point, we know that if there were ID/Key
        // filters in both `self` and `other`, then they intersected to form a consistent set of
        // constraints, so it is safe to interpret the lack of any ID/Key filters respectively as a
        // lack of that kind of constraint, rather than a constraint on the empty set.

        let object_ids = {
            let partition: Vec<_> = keys
                .iter()
                .flatten()
                .filter_map(|(id, v)| v.is_none().then_some(*id))
                .collect();

            (!partition.is_empty()).then_some(partition)
        };

        let object_keys = {
            let partition: Vec<_> = keys
                .iter()
                .flatten()
                .filter_map(|(id, v)| {
                    Some(ObjectKey {
                        object_id: *id,
                        version: (*v)?.into(),
                    })
                })
                .collect();

            (!partition.is_empty()).then_some(partition)
        };

        Some(Self {
            type_: intersect!(type_, TypeFilter::intersect)?,
            owner: intersect!(owner, intersect::by_eq)?,
            object_ids,
            object_keys,
        })
    }

    /// Extract the Object ID and Key filters into one combined map from Object IDs in this filter,
    /// to the versions they should have (or None if the filter mentions the ID but no version for
    /// it).
    fn keys(&self) -> Option<BTreeMap<SuiAddress, Option<u64>>> {
        if self.object_keys.is_none() && self.object_ids.is_none() {
            return None;
        }

        Some(BTreeMap::from_iter(
            self.object_keys
                .iter()
                .flatten()
                .map(|key| (key.object_id, Some(key.version.into())))
                // Chain ID filters after Key filters so if there is overlap, we overwrite the key
                // filter with the ID filter.
                .chain(self.object_ids.iter().flatten().map(|id| (*id, None))),
        ))
    }

    /// Applies ObjectFilter to the input `RawQuery` and returns a new `RawQuery`.
    pub(crate) fn apply(&self, mut query: RawQuery) -> RawQuery {
        // Start by applying the filters on IDs and/or keys because they are combined as
        // a disjunction, while the remaining queries are conjunctions.
        if let Some(object_ids) = &self.object_ids {
            // Maximally strict - match a vec of 0 elements
            if object_ids.is_empty() {
                query = or_filter!(query, "1=0");
            } else {
                let mut inner = String::new();
                let mut prefix = "object_id IN (";
                for id in object_ids {
                    // SAFETY: Writing to a `String` cannot fail.
                    write!(
                        &mut inner,
                        "{prefix}'\\x{}'::bytea",
                        hex::encode(id.into_vec())
                    )
                    .unwrap();
                    prefix = ",";
                }
                inner.push(')');
                query = or_filter!(query, inner);
            }
        }

        if let Some(object_keys) = &self.object_keys {
            // Maximally strict - match a vec of 0 elements
            if object_keys.is_empty() {
                query = or_filter!(query, "1=0");
            } else {
                let mut inner = String::new();
                let mut prefix = "(";
                for ObjectKey { object_id, version } in object_keys {
                    // SAFETY: Writing to a `String` cannot fail.
                    write!(
                        &mut inner,
                        "{prefix}(object_id = '\\x{}'::bytea AND object_version = {})",
                        hex::encode(object_id.into_vec()),
                        version
                    )
                    .unwrap();
                    prefix = " OR ";
                }
                inner.push(')');
                query = or_filter!(query, inner);
            }
        }

        if let Some(owner) = self.owner {
            query = filter!(
                query,
                format!(
                    "owner_id = '\\x{}'::bytea AND owner_type = {}",
                    hex::encode(owner.into_vec()),
                    OwnerType::Address as i16
                )
            );
        }

        if let Some(type_) = &self.type_ {
            return type_.apply_raw(
                query,
                "object_type",
                "object_type_package",
                "object_type_module",
                "object_type_name",
            );
        }

        query
    }

    pub(crate) fn has_filters(&self) -> bool {
        self != &Default::default()
    }
}

impl HistoricalObjectCursor {
    pub(crate) fn new(object_id: Vec<u8>, checkpoint_viewed_at: u64) -> Self {
        Self {
            object_id,
            checkpoint_viewed_at,
        }
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}

impl RawPaginated<Cursor> for StoredHistoryObject {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!(
                "candidates.object_id >= '\\x{}'::bytea",
                hex::encode(cursor.object_id.clone())
            )
        )
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!(
                "candidates.object_id <= '\\x{}'::bytea",
                hex::encode(cursor.object_id.clone())
            )
        )
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        if asc {
            query.order_by("candidates.object_id ASC")
        } else {
            query.order_by("candidates.object_id DESC")
        }
    }
}

impl Target<Cursor> for StoredHistoryObject {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(HistoricalObjectCursor::new(
            self.object_id.clone(),
            checkpoint_viewed_at,
        ))
    }
}

#[async_trait::async_trait]
impl Loader<HistoricalKey> for Db {
    type Value = Object;
    type Error = Error;

    async fn load(&self, keys: &[HistoricalKey]) -> Result<HashMap<HistoricalKey, Object>, Error> {
        use objects_history::dsl;

        let id_versions: BTreeSet<_> = keys
            .iter()
            .map(|key| (key.id.into_vec(), key.version as i64))
            .collect();

        let objects: Vec<StoredHistoryObject> = self
            .execute(move |conn| {
                conn.results(move || {
                    let mut query = dsl::objects_history.into_boxed();

                    // TODO: Speed up using an `obj_version` table.
                    for (id, version) in id_versions.iter().cloned() {
                        query = query
                            .or_filter(dsl::object_id.eq(id).and(dsl::object_version.eq(version)));
                    }

                    query
                })
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch objects: {e}")))?;

        let mut id_version_to_stored = BTreeMap::new();
        for stored in objects {
            let key = (addr(&stored.object_id)?, stored.object_version as u64);
            id_version_to_stored.insert(key, stored);
        }

        let mut result = HashMap::new();
        for key in keys {
            let Some(stored) = id_version_to_stored.get(&(key.id, key.version)) else {
                continue;
            };

            // Filter by key's checkpoint viewed at here. Doing this in memory because it should be
            // quite rare that this query actually filters something, but encoding it in SQL is
            // complicated.
            if key.checkpoint_viewed_at < stored.checkpoint_sequence_number as u64 {
                continue;
            }

            let object = Object::try_from_stored_history_object(
                stored.clone(),
                key.checkpoint_viewed_at,
                // This conversion will use the object's own version as the `Object::root_version`.
                None,
            )?;
            result.insert(*key, object);
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Loader<LatestAtKey> for Db {
    type Value = Object;
    type Error = Error;

    async fn load(&self, keys: &[LatestAtKey]) -> Result<HashMap<LatestAtKey, Object>, Error> {
        // Group keys by checkpoint viewed at and parent version -- we'll issue a separate query for
        // each group.
        #[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
        struct GroupKey {
            checkpoint_viewed_at: u64,
            parent_version: Option<u64>,
        }

        let mut keys_by_cursor_and_parent_version: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for key in keys {
            let group_key = GroupKey {
                checkpoint_viewed_at: key.checkpoint_viewed_at,
                parent_version: key.parent_version,
            };

            keys_by_cursor_and_parent_version
                .entry(group_key)
                .or_default()
                .insert(key.id);
        }

        // Issue concurrent reads for each group of keys.
        let futures = keys_by_cursor_and_parent_version
            .into_iter()
            .map(|(group_key, ids)| {
                self.execute_repeatable(move |conn| {
                    let Some(range) = AvailableRange::result(conn, group_key.checkpoint_viewed_at)?
                    else {
                        return Ok::<Vec<(GroupKey, StoredHistoryObject)>, diesel::result::Error>(
                            vec![],
                        );
                    };

                    let filter = ObjectFilter {
                        object_ids: Some(ids.iter().cloned().collect()),
                        ..Default::default()
                    };

                    // TODO: Implement queries that use a parent version bound using an
                    // `obj_version` table.
                    let apply_parent_bound = |q: RawQuery| {
                        if let Some(parent_version) = group_key.parent_version {
                            filter!(q, format!("object_version <= {parent_version}"))
                        } else {
                            q
                        }
                    };

                    Ok(conn
                        .results(move || {
                            build_objects_query(
                                View::Consistent,
                                range,
                                &Page::bounded(ids.len() as u64),
                                |q| apply_parent_bound(filter.apply(q)),
                                apply_parent_bound,
                            )
                            .into_boxed()
                        })?
                        .into_iter()
                        .map(|r| (group_key, r))
                        .collect())
                })
            });

        // Wait for the reads to all finish, and gather them into the result map.
        let groups = futures::future::join_all(futures).await;

        let mut results = HashMap::new();
        for group in groups {
            for (group_key, stored) in
                group.map_err(|e| Error::Internal(format!("Failed to fetch objects: {e}")))?
            {
                let object = Object::try_from_stored_history_object(
                    stored,
                    group_key.checkpoint_viewed_at,
                    // If `LatestAtKey::parent_version` is set, it must have been correctly
                    // propagated from the `Object::root_version` of some object.
                    group_key.parent_version,
                )?;

                let key = LatestAtKey {
                    id: object.address,
                    checkpoint_viewed_at: group_key.checkpoint_viewed_at,
                    parent_version: group_key.parent_version,
                };

                results.insert(key, object);
            }
        }

        Ok(results)
    }
}

impl From<&ObjectKind> for ObjectStatus {
    fn from(kind: &ObjectKind) -> Self {
        match kind {
            ObjectKind::NotIndexed(_) => ObjectStatus::NotIndexed,
            ObjectKind::Indexed(_, _) => ObjectStatus::Indexed,
            ObjectKind::WrappedOrDeleted(_) => ObjectStatus::WrappedOrDeleted,
        }
    }
}

impl From<&Object> for OwnerImpl {
    fn from(object: &Object) -> Self {
        OwnerImpl {
            address: object.address,
            checkpoint_viewed_at: object.checkpoint_viewed_at,
        }
    }
}

/// Parse a `SuiAddress` from its stored representation.  Failure is an internal error: the
/// database should never contain a malformed address (containing the wrong number of bytes).
fn addr(bytes: impl AsRef<[u8]>) -> Result<SuiAddress, Error> {
    SuiAddress::from_bytes(bytes.as_ref()).map_err(|e| {
        let bytes = bytes.as_ref().to_vec();
        Error::Internal(format!("Error deserializing address: {bytes:?}: {e}"))
    })
}

pub(crate) async fn deserialize_move_struct(
    move_object: &NativeMoveObject,
    resolver: &PackageResolver,
) -> Result<(StructTag, MoveStruct), Error> {
    let struct_tag = StructTag::from(move_object.type_().clone());
    let contents = move_object.contents();
    let move_type_layout = resolver
        .type_layout(TypeTag::from(struct_tag.clone()))
        .await
        .map_err(|e| {
            Error::Internal(format!(
                "Error fetching layout for type {}: {e}",
                struct_tag.to_canonical_string(/* with_prefix */ true)
            ))
        })?;

    let MoveTypeLayout::Struct(layout) = move_type_layout else {
        return Err(Error::Internal("Object is not a move struct".to_string()));
    };

    // TODO (annotated-visitor): Use custom visitors for extracting a dynamic field, and for
    // creating a GraphQL MoveValue directly (not via an annotated visitor).
    let move_struct = BoundedVisitor::deserialize_struct(contents, &layout).map_err(|e| {
        Error::Internal(format!(
            "Error deserializing move struct for type {}: {e}",
            struct_tag.to_canonical_string(/* with_prefix */ true)
        ))
    })?;

    Ok((struct_tag, move_struct))
}

/// Constructs a raw query to fetch objects from the database. Objects are filtered out if they
/// satisfy the criteria but have a later version in the same checkpoint. If object keys are
/// provided, or no filters are specified at all, then this final condition is not applied.
fn objects_query(filter: &ObjectFilter, range: AvailableRange, page: &Page<Cursor>) -> RawQuery
where
{
    if let (Some(_), Some(_)) = (&filter.object_ids, &filter.object_keys) {
        // If both object IDs and object keys are specified, then we need to query in
        // both historical and consistent views, and then union the results.
        let ids_only_filter = ObjectFilter {
            object_keys: None,
            ..filter.clone()
        };
        let (id_query, id_bindings) = build_objects_query(
            View::Consistent,
            range,
            page,
            move |query| ids_only_filter.apply(query),
            move |newer| newer,
        )
        .finish();

        let keys_only_filter = ObjectFilter {
            object_ids: None,
            ..filter.clone()
        };
        let (key_query, key_bindings) = build_objects_query(
            View::Historical,
            range,
            page,
            move |query| keys_only_filter.apply(query),
            move |newer| newer,
        )
        .finish();

        RawQuery::new(
            format!(
                "SELECT * FROM (({id_query}) UNION ALL ({key_query})) AS candidates",
                id_query = id_query,
                key_query = key_query,
            ),
            id_bindings.into_iter().chain(key_bindings).collect(),
        )
        .order_by("object_id")
        .limit(page.limit() as i64)
    } else {
        // Only one of object IDs or object keys is specified, or neither are specified.
        let view = if filter.object_keys.is_some() || !filter.has_filters() {
            View::Historical
        } else {
            View::Consistent
        };

        build_objects_query(
            view,
            range,
            page,
            move |query| filter.apply(query),
            move |newer| newer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_owner_filter_intersection() {
        let f0 = ObjectFilter {
            owner: Some(SuiAddress::from_str("0x1").unwrap()),
            ..Default::default()
        };

        let f1 = ObjectFilter {
            owner: Some(SuiAddress::from_str("0x2").unwrap()),
            ..Default::default()
        };

        assert_eq!(f0.clone().intersect(f0.clone()), Some(f0.clone()));
        assert_eq!(f0.clone().intersect(f1.clone()), None);
    }

    #[test]
    fn test_key_filter_intersection() {
        let i1 = SuiAddress::from_str("0x1").unwrap();
        let i2 = SuiAddress::from_str("0x2").unwrap();
        let i3 = SuiAddress::from_str("0x3").unwrap();
        let i4 = SuiAddress::from_str("0x4").unwrap();

        let f0 = ObjectFilter {
            object_ids: Some(vec![i1, i3]),
            object_keys: Some(vec![
                ObjectKey {
                    object_id: i2,
                    version: 1.into(),
                },
                ObjectKey {
                    object_id: i4,
                    version: 2.into(),
                },
            ]),
            ..Default::default()
        };

        let f1 = ObjectFilter {
            object_ids: Some(vec![i1, i2]),
            object_keys: Some(vec![ObjectKey {
                object_id: i4,
                version: 2.into(),
            }]),
            ..Default::default()
        };

        let f2 = ObjectFilter {
            object_ids: Some(vec![i1, i3]),
            ..Default::default()
        };

        let f3 = ObjectFilter {
            object_keys: Some(vec![
                ObjectKey {
                    object_id: i2,
                    version: 2.into(),
                },
                ObjectKey {
                    object_id: i4,
                    version: 2.into(),
                },
            ]),
            ..Default::default()
        };

        assert_eq!(
            f0.clone().intersect(f1.clone()),
            Some(ObjectFilter {
                object_ids: Some(vec![i1]),
                object_keys: Some(vec![
                    ObjectKey {
                        object_id: i2,
                        version: 1.into(),
                    },
                    ObjectKey {
                        object_id: i4,
                        version: 2.into(),
                    },
                ]),
                ..Default::default()
            })
        );

        assert_eq!(
            f1.clone().intersect(f2.clone()),
            Some(ObjectFilter {
                object_ids: Some(vec![i1]),
                ..Default::default()
            })
        );

        assert_eq!(
            f1.clone().intersect(f3.clone()),
            Some(ObjectFilter {
                object_keys: Some(vec![
                    ObjectKey {
                        object_id: i2,
                        version: 2.into(),
                    },
                    ObjectKey {
                        object_id: i4,
                        version: 2.into(),
                    },
                ]),
                ..Default::default()
            })
        );

        // i2 got a conflicting version assignment
        assert_eq!(f0.clone().intersect(f3.clone()), None);

        // No overlap between these two.
        assert_eq!(f2.clone().intersect(f3.clone()), None);
    }
}
