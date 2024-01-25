// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use super::balance::{self, Balance};
use super::big_int::BigInt;
use super::coin::Coin;
use super::coin_metadata::CoinMetadata;
use super::cursor::{self, Page, Paginated, Target};
use super::digest::Digest;
use super::display::{Display, DisplayEntry};
use super::dynamic_field::{DynamicField, DynamicFieldName};
use super::move_object::MoveObject;
use super::move_package::MovePackage;
use super::owner::OwnerImpl;
use super::stake::StakedSui;
use super::suins_registration::SuinsRegistration;
use super::transaction_block;
use super::transaction_block::TransactionBlockFilter;
use super::type_filter::{ExactTypeFilter, TypeFilter};
use super::{owner::Owner, sui_address::SuiAddress, transaction_block::TransactionBlock};
use crate::context_data::package_cache::PackageCache;
use crate::data::{self, Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::types::base64::Base64;
use crate::types::checkpoint::Checkpoint;
use crate::types::intersect;
use async_graphql::connection::{CursorType, Edge};
use async_graphql::{connection::Connection, *};
use diesel::{
    BoolExpressionMethods, CombineDsl, ExpressionMethods, NullableExpressionMethods,
    OptionalExtension, QueryDsl,
};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::StructTag;
use sui_indexer::models_v2::objects::{
    StoredDeletedHistoryObject, StoredHistoryObject, StoredObject,
};
use sui_indexer::schema_v2::{objects, objects_history, objects_snapshot};
use sui_indexer::types_v2::ObjectStatus as NativeObjectStatus;
use sui_indexer::types_v2::OwnerType;
use sui_package_resolver::Resolver;
use sui_types::object::{
    MoveObject as NativeMoveObject, Object as NativeObject, Owner as NativeOwner,
};
use sui_types::TypeTag;

#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,
    pub kind: ObjectKind,
    // The checkpoint_sequence_number at which this was viewed at, or None if the data was requested
    // at the latest checkpoint.
    pub checkpoint_viewed_at: Option<u64>,
}

/// Type to implement GraphQL fields that are shared by all Objects.
pub(crate) struct ObjectImpl<'o>(pub &'o Object);

#[derive(Clone, Debug)]
pub(crate) enum ObjectKind {
    /// An object loaded from serialized data, such as the contents of a transaction.
    NotIndexed(NativeObject),
    /// An object fetched from the live objects table.
    Live(NativeObject, StoredObject),
    /// An object fetched from the snapshot or historical objects table.
    Historical(NativeObject, StoredHistoryObject),
    /// The object is wrapped or deleted and only partial information can be loaded from the
    /// indexer.
    WrappedOrDeleted(StoredDeletedHistoryObject),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[graphql(name = "ObjectKind")]
pub enum ObjectStatus {
    /// The object is loaded from serialized data, such as the contents of a transaction.
    NotIndexed,
    /// The object is currently live and is not deleted or wrapped.
    Live,
    /// The object is referenced at some version, and thus is fetched from the snapshot or
    /// historical objects table.
    Historical,
    /// The object is deleted or wrapped and only partial information can be loaded from the
    /// indexer.
    WrappedOrDeleted,
}

#[derive(Clone, Debug, PartialEq, Eq, InputObject)]
pub(crate) struct ObjectRef {
    /// ID of the object.
    pub address: SuiAddress,
    /// Version or sequence number of the object.
    pub version: u64,
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
    pub version: u64,
}

/// The object's owner type: Immutable, Shared, Parent, or Address.
#[derive(Union, Clone)]
pub enum ObjectOwner {
    Immutable(Immutable),
    Shared(Shared),
    Parent(Parent),
    Address(AddressOwner),
}

/// An immutable object is an object that can't be mutated, transferred, or deleted.
/// Immutable objects have no owner, so anyone can use them.
#[derive(SimpleObject, Clone)]
pub struct Immutable {
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// A shared object is an object that is shared using the 0x2::transfer::share_object function.
/// Unlike owned objects, once an object is shared, it stays mutable and is accessible by anyone.
#[derive(SimpleObject, Clone)]
pub struct Shared {
    initial_shared_version: u64,
}

/// If the object's owner is a Parent, this object is part of a dynamic field (it is the value of
/// the dynamic field, or the intermediate Field object itself). Also note that if the owner
/// is a parent, then it's guaranteed to be an object.
#[derive(SimpleObject, Clone)]
pub struct Parent {
    parent: Option<Object>,
}

/// An address-owned object is owned by a specific 32-byte address that is
/// either an account address (derived from a particular signature scheme) or
/// an object ID. An address-owned object is accessible only to its owner and no others.
#[derive(SimpleObject, Clone)]
pub struct AddressOwner {
    owner: Option<Owner>,
}

#[allow(dead_code)]
pub(crate) enum ObjectLookupKey {
    Latest,
    LatestAt(u64),
    VersionAt {
        version: u64,
        checkpoint_viewed_at: Option<u64>,
    },
}

pub(crate) type Cursor = cursor::BcsCursor<Vec<u8>>;
type Query<ST, GB> = data::Query<ST, objects::table, GB>;

/// Interface implemented by on-chain values that are addressable by an ID (also referred to as its
/// address). This includes Move objects and packages.
#[derive(Interface)]
#[graphql(
    name = "IObject",
    field(name = "version", ty = "u64"),
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
    pub(crate) async fn default_suins_name(&self, ctx: &Context<'_>) -> Result<Option<String>> {
        OwnerImpl::from(self).default_suins_name(ctx).await
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

    pub(crate) async fn version(&self) -> u64 {
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
        OwnerImpl::from(self).dynamic_field(ctx, name).await
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
        OwnerImpl::from(self).dynamic_object_field(ctx, name).await
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
            .dynamic_fields(ctx, first, after, last, before)
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
    pub(crate) async fn version(&self) -> u64 {
        self.0.version_impl()
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
                    }),
                }))
            }
            O::Immutable => Some(ObjectOwner::Immutable(Immutable { dummy: None })),
            O::ObjectOwner(address) => {
                let parent = Object::query(
                    ctx.data_unchecked(),
                    address.into(),
                    ObjectLookupKey::Latest,
                )
                .await
                .ok()
                .flatten();

                Some(ObjectOwner::Parent(Parent { parent }))
            }
            O::Shared {
                initial_shared_version,
            } => Some(ObjectOwner::Shared(Shared {
                initial_shared_version: initial_shared_version.value(),
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

        TransactionBlock::query(ctx.data_unchecked(), digest.into())
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

        TransactionBlock::paginate(ctx.data_unchecked(), page, filter)
            .await
            .extend()
    }

    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        use ObjectKind as K;
        Ok(match &self.0.kind {
            K::WrappedOrDeleted(_) => None,
            K::Live(_, stored) => Some(Base64::from(&stored.serialized_object)),

            // WrappedOrDeleted objects are also read from the historical objects table, and they do
            // not have a serialized object, so the column is also nullable for stored historical
            // objects.
            K::Historical(_, stored) => stored.serialized_object.as_ref().map(Base64::from),

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
    pub(crate) fn from_native(
        address: SuiAddress,
        native: NativeObject,
        checkpoint_viewed_at: Option<u64>,
    ) -> Object {
        Object {
            address,
            kind: ObjectKind::NotIndexed(native),
            checkpoint_viewed_at,
        }
    }

    pub(crate) fn native_impl(&self) -> Option<&NativeObject> {
        use ObjectKind as K;

        match &self.kind {
            K::Live(native, _) | K::NotIndexed(native) | K::Historical(native, _) => Some(native),
            K::WrappedOrDeleted(_) => None,
        }
    }

    pub(crate) fn version_impl(&self) -> u64 {
        use ObjectKind as K;

        match &self.kind {
            K::Live(native, _) | K::NotIndexed(native) | K::Historical(native, _) => {
                native.version().value()
            }
            K::WrappedOrDeleted(stored) => stored.object_version as u64,
        }
    }

    /// Query the database for a `page` of objects, optionally `filter`-ed.
    ///
    /// The `checkpoint_viewed_at` parameter is an Option<u64> representing the
    /// checkpoint_sequence_number at which this page was queried for, or None if the data was
    /// requested at the latest checkpoint. Each entity returned in the connection will inherit this
    /// checkpoint, so that when viewing that entity's state, it will be from the reference of this
    /// checkpoint_viewed_at parameter.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        filter: ObjectFilter,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Connection<String, Object>, Error> {
        Self::paginate_subtype(db, page, filter, checkpoint_viewed_at, Ok).await
    }

    /// Query the database for a `page` of some sub-type of Object. The page uses the bytes of an
    /// Object ID as the cursor, and can optionally be further `filter`-ed. The subtype is created
    /// using the `downcast` function, which is allowed to fail, if the downcast has failed.
    ///
    /// The `checkpoint_viewed_at` parameter is an Option<u64> representing the
    /// checkpoint_sequence_number at which this page was queried for, or None if the data was
    /// requested at the latest checkpoint. Each entity returned in the connection will inherit this
    /// checkpoint, so that when viewing that entity's state, it will be from the reference of this
    /// checkpoint_viewed_at parameter.
    pub(crate) async fn paginate_subtype<T: OutputType>(
        db: &Db,
        page: Page<Cursor>,
        filter: ObjectFilter,
        checkpoint_viewed_at: Option<u64>,
        downcast: impl Fn(Object) -> Result<T, Error>,
    ) -> Result<Connection<String, T>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredObject, _, _, _>(conn, move || {
                    use objects::dsl;
                    let mut query = dsl::objects.into_boxed();

                    // Start by applying the filters on IDs and/or keys because they are combined as
                    // a disjunction, while the remaining queries are conjunctions.
                    if let Some(object_ids) = &filter.object_ids {
                        query = query.or_filter(
                            dsl::object_id.eq_any(object_ids.iter().map(|a| a.into_vec())),
                        );
                    }

                    for ObjectKey { object_id, version } in filter.object_keys.iter().flatten() {
                        query = query.or_filter(
                            dsl::object_id
                                .eq(object_id.into_vec())
                                .and(dsl::object_version.eq(*version as i64)),
                        );
                    }

                    if let Some(type_) = &filter.type_ {
                        query = query.filter(dsl::object_type.is_not_null());
                        query = type_.apply(query, dsl::object_type.assume_not_null());
                    }

                    if let Some(owner) = &filter.owner {
                        query = query.filter(dsl::owner_id.eq(owner.into_vec()));

                        // If we are supplying an address as the owner, we know that the object must
                        // be owned by an address, or by an object.
                        query = query.filter(dsl::owner_type.eq(OwnerType::Address as i16));
                    }

                    query
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let object = Self::try_from_stored_object(stored, checkpoint_viewed_at)?;
            conn.edges.push(Edge::new(cursor, downcast(object)?));
        }

        Ok(conn)
    }

    /// Query for the object at a specific version, at the checkpoint_viewed_at if given, else
    /// against the latest checkpoint.
    async fn query_at_version(
        db: &Db,
        address: SuiAddress,
        version: u64,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use objects_history::dsl as history;
        use objects_snapshot::dsl as snapshot;

        let version = version as i64;

        let stored_objs: Option<Vec<StoredHistoryObject>> = db
            .execute_repeatable(move |conn| {
                let (lhs, mut rhs) = Checkpoint::available_range(conn)?;

                if let Some(checkpoint_viewed_at) = checkpoint_viewed_at {
                    if checkpoint_viewed_at < lhs || rhs < checkpoint_viewed_at {
                        return Ok(None);
                    }
                    rhs = checkpoint_viewed_at;
                }

                conn.results(move || {
                    // If an object was created or mutated in a checkpoint outside the current
                    // available range, and never touched again, it will not show up in the
                    // objects_history table. Thus, we always need to check the objects_snapshot
                    // table as well.
                    let snapshot_query = snapshot::objects_snapshot
                        .filter(snapshot::object_id.eq(address.into_vec()))
                        .filter(snapshot::object_version.eq(version));

                    let historical_query = history::objects_history
                        .filter(history::object_id.eq(address.into_vec()))
                        .filter(history::object_version.eq(version))
                        .filter(history::checkpoint_sequence_number.between(lhs as i64, rhs as i64))
                        .order_by(history::object_version.desc())
                        .limit(1);

                    snapshot_query.union(historical_query)
                })
                .optional() // Return optional to match the state when checkpoint_viewed_at is out of range
            })
            .await?;

        let Some(stored_objs) = stored_objs else {
            return Ok(None);
        };

        // Select the max by key after the union query, because Diesel currently does not support order_by on union
        stored_objs
            .into_iter()
            .max_by_key(|o| o.object_version)
            .map(|obj| Self::try_from_stored_history_object(obj, checkpoint_viewed_at))
            .transpose()
    }

    /// Query for the object at the latest version at the checkpoint sequence number if given, else
    /// the latest version of the object against the latest checkpoint.
    async fn query_latest_at_checkpoint(
        db: &Db,
        address: SuiAddress,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use objects_history::dsl as history;
        use objects_snapshot::dsl as snapshot;

        let stored_objs: Option<Vec<StoredHistoryObject>> = db
            .execute_repeatable(move |conn| {
                let (lhs, mut rhs) = Checkpoint::available_range(conn)?;

                if let Some(checkpoint_viewed_at) = checkpoint_viewed_at {
                    if checkpoint_viewed_at < lhs || rhs < checkpoint_viewed_at {
                        return Ok(None);
                    }
                    rhs = checkpoint_viewed_at;
                }

                conn.results(move || {
                    // If an object was created or mutated in a checkpoint outside the current
                    // available range, and never touched again, it will not show up in the
                    // objects_history table. Thus, we always need to check the objects_snapshot
                    // table as well.
                    let snapshot_query = snapshot::objects_snapshot
                        .filter(snapshot::object_id.eq(address.into_vec()));

                    let historical_query = history::objects_history
                        .filter(history::object_id.eq(address.into_vec()))
                        .filter(history::checkpoint_sequence_number.between(lhs as i64, rhs as i64))
                        .order_by(history::object_version.desc())
                        .limit(1);

                    snapshot_query.union(historical_query)
                })
                .optional() // Return optional to match the state when checkpoint_viewed_at is out of range
            })
            .await?;

        let Some(stored_objs) = stored_objs else {
            return Ok(None);
        };

        // Select the max by key after the union query, because Diesel currently does not support order_by on union
        stored_objs
            .into_iter()
            .max_by_key(|o| o.object_version)
            .map(|obj| Self::try_from_stored_history_object(obj, checkpoint_viewed_at))
            .transpose()
    }

    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        key: ObjectLookupKey,
    ) -> Result<Option<Self>, Error> {
        match key {
            ObjectLookupKey::Latest => Self::query_latest_at_checkpoint(db, address, None).await,
            ObjectLookupKey::LatestAt(checkpoint_sequence_number) => {
                Self::query_latest_at_checkpoint(db, address, Some(checkpoint_sequence_number))
                    .await
            }
            ObjectLookupKey::VersionAt {
                version,
                checkpoint_viewed_at,
            } => Self::query_at_version(db, address, version, checkpoint_viewed_at).await,
        }
        .map_err(|e| Error::Internal(format!("Failed to fetch object: {e}")))
    }

    /// Query for a singleton object identified by its type. Note: the object is assumed to be a
    /// singleton (we either find at least one object with this type and then return it, or return
    /// nothing).
    pub(crate) async fn query_singleton(db: &Db, type_: TypeTag) -> Result<Option<Object>, Error> {
        use objects::dsl;

        let stored_obj: Option<StoredObject> = db
            .execute(move |conn| {
                conn.first(move || {
                    dsl::objects.filter(
                        dsl::object_type.eq(type_.to_canonical_string(/* with_prefix */ true)),
                    )
                })
                .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch singleton: {e}")))?;

        stored_obj
            .map(|obj| Object::try_from_stored_object(obj, None))
            .transpose()
    }

    pub(crate) fn try_from_stored_object(
        stored_object: StoredObject,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Self, Error> {
        let address = addr(&stored_object.object_id)?;
        let native_object = bcs::from_bytes(&stored_object.serialized_object)
            .map_err(|_| Error::Internal(format!("Failed to deserialize object {address}")))?;

        Ok(Self {
            address,
            kind: ObjectKind::Live(native_object, stored_object),
            checkpoint_viewed_at,
        })
    }

    pub(crate) fn try_from_stored_history_object(
        history_object: StoredHistoryObject,
        checkpoint_viewed_at: Option<u64>,
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

                Ok(Self {
                    address,
                    kind: ObjectKind::Historical(native_object, history_object),
                    checkpoint_viewed_at,
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
            }),
        }
    }
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
                        version: (*v)?,
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
                .map(|key| (key.object_id, Some(key.version)))
                // Chain ID filters after Key filters so if there is overlap, we overwrite the key
                // filter with the ID filter.
                .chain(self.object_ids.iter().flatten().map(|id| (*id, None))),
        ))
    }
}

impl Paginated<Cursor> for StoredObject {
    type Source = objects::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(objects::dsl::object_id.ge((**cursor).clone()))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(objects::dsl::object_id.le((**cursor).clone()))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use objects::dsl;
        if asc {
            query.order_by(dsl::object_id.asc())
        } else {
            query.order_by(dsl::object_id.desc())
        }
    }
}

impl Target<Cursor> for StoredObject {
    fn cursor(&self) -> Cursor {
        Cursor::new(self.object_id.clone())
    }
}

impl From<&ObjectKind> for ObjectStatus {
    fn from(kind: &ObjectKind) -> Self {
        match kind {
            ObjectKind::NotIndexed(_) => ObjectStatus::NotIndexed,
            ObjectKind::Live(_, _) => ObjectStatus::Live,
            ObjectKind::Historical(_, _) => ObjectStatus::Historical,
            ObjectKind::WrappedOrDeleted(_) => ObjectStatus::WrappedOrDeleted,
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
    resolver: &Resolver<PackageCache>,
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

    let move_struct = MoveStruct::simple_deserialize(contents, &layout).map_err(|e| {
        Error::Internal(format!(
            "Error deserializing move struct for type {}: {e}",
            struct_tag.to_canonical_string(/* with_prefix */ true)
        ))
    })?;

    Ok((struct_tag, move_struct))
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
                    version: 1,
                },
                ObjectKey {
                    object_id: i4,
                    version: 2,
                },
            ]),
            ..Default::default()
        };

        let f1 = ObjectFilter {
            object_ids: Some(vec![i1, i2]),
            object_keys: Some(vec![ObjectKey {
                object_id: i4,
                version: 2,
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
                    version: 2,
                },
                ObjectKey {
                    object_id: i4,
                    version: 2,
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
                        version: 1
                    },
                    ObjectKey {
                        object_id: i4,
                        version: 2
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
                        version: 2
                    },
                    ObjectKey {
                        object_id: i4,
                        version: 2
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
