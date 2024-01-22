// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Deref;

use crate::types::intersect;
use async_graphql::connection::{CursorType, Edge};
use async_graphql::{connection::Connection, *};
use diesel::{
    BoolExpressionMethods, CombineDsl, ExpressionMethods, NullableExpressionMethods,
    OptionalExtension, QueryDsl,
};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_indexer::models_v2::objects::{
    StoredDeletedHistoryObject, StoredHistoryObject, StoredObject,
};
use sui_indexer::schema_v2::{checkpoints, objects, objects_history, objects_snapshot};
use sui_indexer::types_v2::ObjectStatus as NativeObjectStatus;
use sui_indexer::types_v2::OwnerType;
use sui_json_rpc::name_service::NameServiceConfig;
use sui_package_resolver::Resolver;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::gas_coin::GAS;
use sui_types::TypeTag;

use super::balance;
use super::big_int::BigInt;
use super::cursor::{self, Page, Paginated, RawPaginated, Target};
use super::display::{get_rendered_fields, DisplayEntry};
use super::dynamic_field::{DynamicField, DynamicFieldName};
use super::move_object::MoveObject;
use super::move_package::MovePackage;
use super::suins_registration::SuinsRegistration;
use super::type_filter::{ExactTypeFilter, TypeFilter};
use super::{
    balance::Balance, coin::Coin, owner::Owner, stake::StakedSui, sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};
use crate::context_data::db_data_provider::PgManager;
use crate::context_data::package_cache::PackageCache;
use crate::data::{self, Db, DbConnection, QueryExecutor, RawSqlQuery};
use crate::error::Error;
use crate::types::base64::Base64;
use sui_types::object::{
    MoveObject as NativeMoveObject, Object as NativeObject, Owner as NativeOwner,
};

#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,
    pub kind: ObjectKind,
}

#[derive(Clone, Debug)]
pub(crate) enum ObjectKind {
    /// An object loaded from serialized data, such as the contents of a transaction. This is used
    /// to represent system packages that are written before the new epoch starts, or from the
    /// genesis transaction.
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
pub(crate) enum ObjectVersionKey {
    Latest,
    LatestAt(u64),   // checkpoint_sequence_number
    Historical(u64), // version
}

pub(crate) enum HistoricalObjectPaginationResult<I, T>
where
    I: Iterator<Item = T>,
{
    Error(HistoricalObjectPaginationError),
    Success(bool, bool, I),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum HistoricalObjectPaginationError {
    #[error(
        "The requested checkpoint sequence number {0} is outside the available range: [{1}, {2}]"
    )]
    OutsideAvailableRange(u64, u64, u64),
}

pub(crate) type Cursor = cursor::BcsCursor<HistoricalObjectCursor>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct HistoricalObjectCursor {
    object_id: Vec<u8>,
    checkpoint_sequence_number: u64,
}
type Query<ST, GB> = data::Query<ST, objects::table, GB>;

/// An object in Sui is a package (set of Move bytecode modules) or object (typed data structure
/// with fields) with additional metadata detailing its id, version, transaction digest, owner
/// field indicating how this object can be accessed.
#[Object]
impl Object {
    async fn version(&self) -> Option<u64> {
        self.version_impl()
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// - NOT_INDEXED: the object is loaded from serialized data, such as the contents of a
    ///   transaction.
    /// - LIVE: the object is currently live and is not deleted or wrapped.
    /// - HISTORICAL: the object is referenced at some version, and thus is fetched from the
    ///   snapshot or historical objects table.
    /// - WRAPPED_OR_DELETED: The object is deleted or wrapped and only partial information can be
    ///   loaded from the indexer.
    async fn status(&self) -> ObjectStatus {
        ObjectStatus::from(&self.kind)
    }

    /// 32-byte hash that identifies the object's current contents, encoded as a Base58 string.
    async fn digest(&self) -> Option<String> {
        self.native_impl()
            .map(|native| native.digest().base58_encode())
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated.
    /// This number is recalculated based on the present storage gas price.
    async fn storage_rebate(&self) -> Option<BigInt> {
        self.native_impl()
            .map(|native| BigInt::from(native.storage_rebate))
    }

    /// The set of named templates defined on-chain for the type of this object,
    /// to be handled off-chain. The server substitutes data from the object
    /// into these templates to generate a display string per template.
    async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        let Some(native) = self.native_impl() else {
            return Ok(None);
        };

        let resolver: &Resolver<PackageCache> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;
        let move_object = native
            .data
            .try_as_move()
            .ok_or_else(|| Error::Internal("Failed to convert object into MoveObject".to_string()))
            .extend()?;

        let (struct_tag, move_struct) = deserialize_move_struct(move_object, resolver)
            .await
            .extend()?;

        let stored_display = ctx
            .data_unchecked::<PgManager>()
            .fetch_display_object_by_type(&struct_tag)
            .await
            .extend()?;

        let Some(stored_display) = stored_display else {
            return Ok(None);
        };

        let event = stored_display
            .to_display_update_event()
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;

        Ok(Some(
            get_rendered_fields(event.fields, &move_struct).extend()?,
        ))
    }

    /// The Base64 encoded bcs serialization of the object's content.
    async fn bcs(&self) -> Result<Option<Base64>> {
        use ObjectKind as K;
        Ok(match &self.kind {
            K::WrappedOrDeleted(_) => None,
            K::Live(_, stored) => Some(Base64::from(&stored.serialized_object)),
            // WrappedOrDeleted objects do not have a serialized object, thus this column in the db is nullable.
            K::Historical(_, stored) => stored.serialized_object.as_ref().map(Base64::from),
            K::NotIndexed(native) => {
                let bytes = bcs::to_bytes(native)
                    .map_err(|e| {
                        Error::Internal(format!(
                            "Failed to serialize object at {}: {e}",
                            self.address
                        ))
                    })
                    .extend()?;
                Some(Base64::from(&bytes))
            }
        })
    }

    /// The transaction block that created this version of the object.
    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        let Some(native) = self.native_impl() else {
            return Ok(None);
        };
        let digest = native.previous_transaction;

        TransactionBlock::query(ctx.data_unchecked(), digest.into())
            .await
            .extend()
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    /// Immutable and Shared Objects do not have owners.
    async fn owner(&self, ctx: &Context<'_>) -> Option<ObjectOwner> {
        use NativeOwner as O;

        let Some(native) = self.native_impl() else {
            return None;
        };

        match native.owner {
            O::AddressOwner(address) => {
                let address = SuiAddress::from(address);
                Some(ObjectOwner::Address(AddressOwner {
                    owner: Some(Owner { address }),
                }))
            }
            O::Immutable => Some(ObjectOwner::Immutable(Immutable { dummy: None })),
            O::ObjectOwner(address) => {
                let parent = Object::query(
                    ctx.data_unchecked(),
                    address.into(),
                    ObjectVersionKey::Latest,
                )
                .await
                .ok()
                .flatten();

                return Some(ObjectOwner::Parent(Parent { parent }));
            }
            O::Shared {
                initial_shared_version,
            } => Some(ObjectOwner::Shared(Shared {
                initial_shared_version: initial_shared_version.value(),
            })),
        }
    }

    /// Attempts to convert the object into a MoveObject
    async fn as_move_object(&self) -> Option<MoveObject> {
        MoveObject::try_from(self).ok()
    }

    /// Attempts to convert the object into a MovePackage
    async fn as_move_package(&self) -> Option<MovePackage> {
        MovePackage::try_from(self).ok()
    }

    // =========== Owner interface methods =============

    /// The address of the object, named as such to avoid conflict with the address type.
    pub async fn address(&self) -> SuiAddress {
        self.address
    }

    /// The objects owned by this object
    pub async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<self::Cursor>,
        last: Option<u64>,
        before: Option<self::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
            owner: Some(self.address),
            ..Default::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        Object::paginate(
            ctx.data_unchecked(),
            page,
            None, // TODO (wlmyng): we can't blindly take the object's checkpoint_sequence_number - only do so if the parent object was selected at version
            filter,
        )
        .await
        .extend()
    }

    /// The balance of coin objects of a particular coin type owned by the object.
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

    /// The balances of all coin types owned by this object. Coins of the same type are grouped
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

    /// The coin objects for this object.
    ///
    /// The type field is a string of the inner type of the coin by which to filter (e.g.
    /// `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    pub async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let coin = type_.map_or_else(GAS::type_tag, |t| t.0);
        Coin::paginate(ctx.data_unchecked(), page, coin, Some(self.address))
            .await
            .extend()
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this object.
    pub async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
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

    /// The SuinsRegistration NFTs owned by this object. These grant the owner the capability to
    /// manage the associated domain.
    pub async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
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
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        use DynamicFieldType as T;
        DynamicField::query(ctx.data_unchecked(), self.address, name, T::DynamicField)
            .await
            .extend()
    }

    /// Access a dynamic object field on an object using its name.
    /// Names are arbitrary Move values whose type have `copy`, `drop`, and `store`, and are specified
    /// using their type, and their BCS contents, Base64 encoded.
    /// The value of a dynamic object field can also be accessed off-chain directly via its address (e.g. using `Query.object`).
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        use DynamicFieldType as T;
        DynamicField::query(ctx.data_unchecked(), self.address, name, T::DynamicObject)
            .await
            .extend()
    }

    /// The dynamic fields on an object.
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        DynamicField::paginate(ctx.data_unchecked(), page, self.address)
            .await
            .extend()
    }
}

impl Object {
    /// Construct a GraphQL object from a native object, without its stored (indexed) counterpart.
    pub(crate) fn from_native(address: SuiAddress, native: NativeObject) -> Object {
        Object {
            address,
            kind: ObjectKind::NotIndexed(native),
        }
    }

    pub(crate) fn native_impl(&self) -> Option<&NativeObject> {
        use ObjectKind as K;

        match &self.kind {
            K::Live(native, _) | K::NotIndexed(native) | K::Historical(native, _) => Some(native),
            K::WrappedOrDeleted(_) => None,
        }
    }

    pub(crate) fn version_impl(&self) -> Option<u64> {
        use ObjectKind as K;

        match &self.kind {
            K::Live(native, _) | K::NotIndexed(native) | K::Historical(native, _) => {
                Some(native.version().value())
            }
            K::WrappedOrDeleted(stored) => Some(stored.object_version as u64),
        }
    }

    /// Query the database for a `page` of objects, optionally `filter`-ed.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        checkpoint_sequence_number: Option<u64>,
        filter: ObjectFilter,
    ) -> Result<Connection<String, Object>, Error> {
        Self::paginate_subtype(db, page, checkpoint_sequence_number, filter, Ok).await
    }

    /// Query the database for a `page` of some sub-type of Object. The page uses the bytes of an
    /// Object ID as the cursor, and can optionally be further `filter`-ed. The subtype is created
    /// using the `downcast` function, which is allowed to fail, if the downcast has failed.
    pub(crate) async fn paginate_subtype<T: OutputType>(
        db: &Db,
        page: Page<Cursor>,
        checkpoint_sequence_number: Option<u64>,
        filter: ObjectFilter,
        downcast: impl Fn(Object) -> Result<T, Error>,
    ) -> Result<Connection<String, T>, Error> {
        let mut conn: Connection<String, T> = Connection::new(false, false);

        // Regardless of whether the cursor is the upper or lower bound, the cursor's
        // checkpoint_sequence_number identifies the consistent upper bound when it was calculated

        let checkpoint_sequence_number = match validate_cursor_consistency(
            checkpoint_sequence_number,
            page.after(),
            page.before(),
        ) {
            Ok(checkpoint_sequence_number) => checkpoint_sequence_number,
            Err(_) => return Ok(conn),
        };

        let pagination_results = db
            .execute_repeatable(move |conn| {
                use checkpoints::dsl as checkpoints;
                use objects_snapshot::dsl as snapshot;

                // If the checkpoint_sequence_number among cursor(s) and input is consistent, it
                // still needs to be within the graphql's availableRange
                let checkpoint_range: Vec<i64> = conn.results(move || {
                    let rhs = checkpoints::checkpoints
                        .select(checkpoints::sequence_number)
                        .order(checkpoints::sequence_number.desc());

                    let lhs = snapshot::objects_snapshot
                        .select(snapshot::checkpoint_sequence_number)
                        .order(snapshot::checkpoint_sequence_number.desc());

                    lhs.union(rhs)
                })?;

                let lhs: i64 = checkpoint_range.iter().min().copied().unwrap_or(0);
                let mut rhs: i64 = checkpoint_range.iter().max().copied().unwrap_or(0);

                if let Some(checkpoint_sequence_number) = checkpoint_sequence_number {
                    if checkpoint_sequence_number > rhs as u64
                        || checkpoint_sequence_number < lhs as u64
                    {
                        return Ok::<_, diesel::result::Error>(
                            HistoricalObjectPaginationResult::Error(
                                HistoricalObjectPaginationError::OutsideAvailableRange(
                                    checkpoint_sequence_number,
                                    lhs as u64,
                                    rhs as u64,
                                ),
                            ),
                        );
                    }
                    rhs = checkpoint_sequence_number as i64;
                }

                let result = page.paginate_consistent_query::<StoredHistoryObject, _, _>(
                    conn,
                    move |element| {
                        element.map(|obj| {
                            Cursor::new(HistoricalObjectCursor::new(
                                obj.object_id.clone(),
                                rhs as u64,
                            ))
                        })
                    },
                    move || {
                        let start_cp = lhs;
                        let end_cp = rhs;

                        let mut snapshot_query = RawSqlQuery::new_with_select(
                            "objects_snapshot".to_string(),
                            "SELECT * FROM objects_snapshot".to_string(),
                        );
                        let mut history_query = RawSqlQuery::new_with_select(
                            "objects_history".to_string(),
                            "SELECT * FROM objects_history".to_string(),
                        );

                        Object::raw_object_filter(&mut snapshot_query, &filter);
                        Object::raw_object_filter(&mut history_query, &filter);

                        // Join the two tables together, selecting objects that satisfy the filtering criteria
                        let unioned =
                            format!("{} UNION {}", snapshot_query.build(), history_query.build());

                        // Keep only the latest object_version for each object_id
                        let candidates = format!(
                            r#"
                            SELECT DISTINCT ON (object_id) *
                            FROM ({unioned}) o
                            WHERE checkpoint_sequence_number BETWEEN {start_cp} AND {end_cp}
                            ORDER BY object_id, object_version DESC"#,
                            unioned = unioned,
                            start_cp = start_cp,
                            end_cp = end_cp
                        );

                        let newer = format!(
                            r#"
                            SELECT object_id, object_version
                            FROM objects_history
                            WHERE checkpoint_sequence_number BETWEEN {} AND {}"#,
                            start_cp, end_cp
                        );

                        // This left join checks whether every object that satisfies the filtering criteria
                        // has an even newer version. If it does, drop it from the result set.
                        let final_select = format!(
                            r#"
                            SELECT candidates.* FROM ({}) candidates
                            LEFT JOIN ({}) newer
                            ON (candidates.object_id = newer.object_id
                                AND
                                candidates.object_version < newer.object_version
                            )"#,
                            candidates, newer
                        );

                        let mut query =
                            RawSqlQuery::new_with_select("candidates".to_string(), final_select);

                        query.and_filter("newer.object_version IS NULL".to_string());

                        query
                    },
                )?;

                Ok(HistoricalObjectPaginationResult::Success(
                    result.0, result.1, result.2,
                ))
            })
            .await?;

        let (prev, next, results) = match pagination_results {
            HistoricalObjectPaginationResult::Error(e) => {
                return Err(Error::Client(e.to_string()));
            }
            HistoricalObjectPaginationResult::Success(prev, next, results) => (prev, next, results),
        };

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let object = Object::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, downcast(object)?));
        }

        Ok(conn)
    }

    async fn query_live(db: &Db, address: SuiAddress) -> Result<Option<Self>, Error> {
        use objects::dsl as objects;
        let vec_address = address.into_vec();

        let stored_obj: Option<StoredObject> = db
            .execute(move |conn| {
                conn.first(move || {
                    objects::objects.filter(objects::object_id.eq(vec_address.clone()))
                })
                .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch object: {e}")))?;

        stored_obj.map(Self::try_from).transpose()
    }

    fn raw_object_filter(query: &mut RawSqlQuery, filter: &ObjectFilter) {
        if let Some(object_ids) = &filter.object_ids {
            if object_ids.is_empty() {
                // Maximally strict - match a vec of 0 elements
                query.and_filter("1==0".to_string());
            } else {
                let mut object_id_filter = "object_id IN (".to_string();
                object_id_filter += &object_ids
                    .iter()
                    .map(|id| format!("'\\x{}'::bytea", hex::encode(id.into_vec())))
                    .collect::<Vec<_>>()
                    .join(",");
                object_id_filter += ")";
                query.or_filter(object_id_filter);
            }
        }

        if let Some(object_keys) = &filter.object_keys {
            if object_keys.is_empty() {
                // Maximally strict - match a vec of 0 elements
                query.and_filter("1==0".to_string());
            } else {
                let mut object_key_filter = "(".to_string();
                object_key_filter += &object_keys
                    .iter()
                    .map(|ObjectKey { object_id, version }| {
                        format!(
                            "(object_id = '\\x{}'::bytea AND object_version = {})",
                            hex::encode(object_id.into_vec()),
                            version
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" OR ");
                object_key_filter += ")";
                query.or_filter(object_key_filter);
            }
        }

        if let Some(type_) = &filter.type_ {
            type_.apply_raw(query, "object_type");
        }

        if let Some(owner) = &filter.owner {
            query.and_filter(format!(
                "owner_id = '\\x{}'::bytea",
                hex::encode(owner.into_vec())
            ));
            query.and_filter(format!("owner_type = {}", OwnerType::Address as i16));
        }
    }

    async fn query_at_version(
        db: &Db,
        address: SuiAddress,
        version: u64,
    ) -> Result<Option<Self>, Error> {
        use checkpoints::dsl as checkpoints;
        use objects_history::dsl as history;
        use objects_snapshot::dsl as snapshot;

        let version = version as i64;

        let results: Option<Vec<StoredHistoryObject>> = db
            .execute(move |conn| {
                conn.results(move || {
                    // If an object was created or mutated in a checkpoint outside the current
                    // available range, and never touched again, it will not show up in the
                    // objects_history table. Thus, we always need to check the objects_snapshot
                    // table as well.
                    let snapshot_query = snapshot::objects_snapshot
                        .filter(snapshot::object_id.eq(address.into_vec()))
                        .filter(snapshot::object_version.eq(version))
                        .into_boxed();
                    let mut historical_query = history::objects_history
                        .filter(history::object_id.eq(address.into_vec()))
                        .filter(history::object_version.eq(version))
                        .order_by(history::object_version.desc())
                        .limit(1)
                        .into_boxed();

                    let left = snapshot::objects_snapshot
                        .select(snapshot::checkpoint_sequence_number)
                        .order(snapshot::checkpoint_sequence_number.desc())
                        .limit(1);

                    let right = checkpoints::checkpoints
                        .select(checkpoints::sequence_number)
                        .order(checkpoints::sequence_number.desc())
                        .limit(1);

                    historical_query = historical_query
                        .filter(
                            left.single_value()
                                .is_null()
                                .or(history::checkpoint_sequence_number
                                    .nullable()
                                    .ge(left.single_value())),
                        )
                        .filter(
                            history::checkpoint_sequence_number
                                .nullable()
                                .le(right.single_value()),
                        );

                    snapshot_query.union(historical_query)
                })
                .optional()
            })
            .await?;

        // For the moment, if the object existed at some point, it will have eventually be written
        // to objects_snapshot. Therefore, if both results are None, the object has never existed.
        let Some(stored_objs) = results else {
            return Ok(None);
        };

        // Select the max by key after the union query, because Diesel currently does not support order_by on union
        stored_objs
            .into_iter()
            .max_by_key(|o| o.object_version)
            .map(Self::try_from)
            .transpose()
    }

    async fn query_latest_at_checkpoint(
        db: &Db,
        address: SuiAddress,
        checkpoint_sequence_number: u64,
    ) -> Result<Option<Self>, Error> {
        use objects_history::dsl as history;
        use objects_snapshot::dsl as snapshot;

        let checkpoint_sequence_number = checkpoint_sequence_number as i64;

        let results: Option<Vec<StoredHistoryObject>> = db
            .execute(move |conn| {
                conn.results(move || {
                    // If an object was created or mutated in a checkpoint outside the current
                    // available range, and never touched again, it will not show up in the
                    // objects_history table. Thus, we always need to check the objects_snapshot
                    // table as well.
                    let mut snapshot_query = snapshot::objects_snapshot
                        .filter(snapshot::object_id.eq(address.into_vec()))
                        .into_boxed();

                    let mut historical_query = history::objects_history
                        .filter(history::object_id.eq(address.into_vec()))
                        .order_by(history::object_version.desc())
                        .limit(1)
                        .into_boxed();

                    let left = snapshot::objects_snapshot
                        .select(snapshot::checkpoint_sequence_number)
                        .order(snapshot::checkpoint_sequence_number.desc())
                        .limit(1);

                    historical_query = historical_query.filter(
                        left.single_value()
                            .is_null()
                            .or(history::checkpoint_sequence_number
                                .nullable()
                                .ge(left.single_value())),
                    );

                    historical_query = historical_query
                        .filter(history::checkpoint_sequence_number.le(checkpoint_sequence_number));

                    snapshot_query = snapshot_query.filter(
                        snapshot::checkpoint_sequence_number.le(checkpoint_sequence_number),
                    );

                    snapshot_query.union(historical_query)
                })
                .optional()
            })
            .await?;

        // For the moment, if the object existed at some point, it will have eventually be written
        // to objects_snapshot. Therefore, if both results are None, the object has never existed.
        let Some(stored_objs) = results else {
            return Ok(None);
        };

        // Select the max by key after the union query, because Diesel currently does not support order_by on union
        stored_objs
            .into_iter()
            .max_by_key(|o| o.object_version)
            .map(Self::try_from)
            .transpose()
    }

    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        key: ObjectVersionKey,
    ) -> Result<Option<Self>, Error> {
        match key {
            ObjectVersionKey::Latest => Self::query_live(db, address).await,
            ObjectVersionKey::LatestAt(checkpoint_sequence_number) => {
                Self::query_latest_at_checkpoint(db, address, checkpoint_sequence_number).await
            }
            ObjectVersionKey::Historical(version) => {
                Self::query_at_version(db, address, version).await
            }
        }
        .map_err(|e| Error::Internal(format!("Failed to fetch object: {e}")))
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
        query.filter(objects::dsl::object_id.ge(cursor.object_id.clone()))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(objects::dsl::object_id.le(cursor.object_id.clone()))
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

impl RawPaginated<Cursor> for StoredHistoryObject {
    fn filter_ge(cursor: &Cursor, query: &mut RawSqlQuery) {
        query.and_filter(format!(
            "{}.object_id >= '\\x{}'::bytea",
            query.alias,
            hex::encode(cursor.object_id.clone())
        ))
    }

    fn filter_le(cursor: &Cursor, query: &mut RawSqlQuery) {
        query.and_filter(format!(
            "{}.object_id <= '\\x{}'::bytea",
            query.alias,
            hex::encode(cursor.object_id.clone())
        ))
    }

    fn order(asc: bool, query: &mut RawSqlQuery) {
        query.order_by(&format!("{}.object_id", query.alias), asc);
    }
}

impl Target<Cursor> for StoredObject {
    fn cursor(&self) -> Cursor {
        Cursor::new(HistoricalObjectCursor {
            object_id: self.object_id.clone(),
            checkpoint_sequence_number: self.checkpoint_sequence_number as u64,
        })
    }
}

impl Target<Cursor> for StoredHistoryObject {
    fn cursor(&self) -> Cursor {
        Cursor::new(HistoricalObjectCursor {
            object_id: self.object_id.clone(),
            checkpoint_sequence_number: self.checkpoint_sequence_number as u64,
        })
    }
}

impl HistoricalObjectCursor {
    pub(crate) fn new(object_id: Vec<u8>, checkpoint_sequence_number: u64) -> Self {
        Self {
            object_id,
            checkpoint_sequence_number,
        }
    }
}

impl TryFrom<StoredObject> for Object {
    type Error = Error;

    fn try_from(stored_object: StoredObject) -> Result<Self, Error> {
        let address = addr(&stored_object.object_id)?;
        let native_object = bcs::from_bytes(&stored_object.serialized_object)
            .map_err(|_| Error::Internal(format!("Failed to deserialize object {address}")))?;

        Ok(Self {
            address,
            kind: ObjectKind::Live(native_object, stored_object),
        })
    }
}

impl TryFrom<StoredHistoryObject> for Object {
    type Error = Error;

    fn try_from(history_object: StoredHistoryObject) -> Result<Self, Error> {
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
            }),
        }
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

/// Check that the cursors, if provided, have the same checkpoint_sequence_number.
pub(crate) fn validate_cursor_consistency(
    checkpoint_sequence_number: Option<u64>,
    after: Option<&Cursor>,
    before: Option<&Cursor>,
) -> Result<Option<u64>, Error> {
    let options = [
        after.map(|after| after.deref().checkpoint_sequence_number),
        before.map(|before| before.deref().checkpoint_sequence_number),
        checkpoint_sequence_number,
    ];

    let mut values = options.iter().flatten();

    let checkpoint_sequence_number = if let Some(first_val) = values.next() {
        if values.all(|val| val == first_val) {
            Ok(Some(*first_val))
        } else {
            Err(Error::Client("Inconsistent cursor".to_string()))
        }
    } else {
        Ok(None)
    }?;

    Ok(checkpoint_sequence_number)
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

    #[test]
    fn test_validate_cursor_consistency_all_none() {
        let result = validate_cursor_consistency(None, None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_validate_cursor_consistency_all_same() {
        let obj1 = SuiAddress::from_str("0x1").unwrap();
        let obj2 = SuiAddress::from_str("0x2").unwrap();

        let result = validate_cursor_consistency(
            Some(1),
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj1.into_vec(),
                1,
            ))),
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj2.into_vec(),
                1,
            ))),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(1));
    }

    #[test]
    fn test_validate_cursor_consistency_only_after() {
        let obj1 = SuiAddress::from_str("0x1").unwrap();

        let result = validate_cursor_consistency(
            None,
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj1.into_vec(),
                1,
            ))),
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(1));
    }

    #[test]
    fn test_validate_cursor_consistency_only_input() {
        let result = validate_cursor_consistency(Some(1), None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(1));
    }

    #[test]
    fn test_validate_cursor_consistency_cursor_ne_input() {
        let obj1 = SuiAddress::from_str("0x1").unwrap();

        let result = validate_cursor_consistency(
            Some(2),
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj1.into_vec(),
                1,
            ))),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cursor_consistency_after_ne_before() {
        let obj1 = SuiAddress::from_str("0x1").unwrap();
        let obj2 = SuiAddress::from_str("0x2").unwrap();

        let result = validate_cursor_consistency(
            None,
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj1.into_vec(),
                1,
            ))),
            Some(&Cursor::new(HistoricalObjectCursor::new(
                obj2.into_vec(),
                2,
            ))),
        );
        assert!(result.is_err());
    }
}
