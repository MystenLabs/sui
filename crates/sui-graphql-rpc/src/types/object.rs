// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::connection::{CursorType, Edge};
use async_graphql::{connection::Connection, *};
use diesel::{
    BoolExpressionMethods, ExpressionMethods, NullableExpressionMethods, OptionalExtension,
    QueryDsl,
};
use fastcrypto::encoding::{Base58, Encoding};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::StructTag;
use sui_indexer::models_v2::objects::StoredObject;
use sui_indexer::schema_v2::objects;
use sui_indexer::types_v2::OwnerType;
use sui_json_rpc::name_service::NameServiceConfig;
use sui_package_resolver::Resolver;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::gas_coin::GAS;
use sui_types::TypeTag;

use super::big_int::BigInt;
use super::cursor::{self, Page, Target};
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
use crate::data::{self, Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::types::base64::Base64;
use crate::types::intersect;
use sui_types::object::{
    MoveObject as NativeMoveObject, Object as NativeObject, Owner as NativeOwner,
};

#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,

    /// Representation of an Object in the Indexer's Store.
    pub stored: Option<StoredObject>,

    /// Deserialized representation of `stored_object.serialized_object`.
    pub native: NativeObject,
}

/// Previous implementation of `ObjectFilter`, kept around to use in the legacy DB provider APIs,
/// while we are in the process of migrating over to the DB APIs.
#[derive(Default, Clone)]
pub(crate) struct DeprecatedObjectFilter {
    /// This field is used to specify the type of objects that should be included in the query
    /// results.
    ///
    /// Objects can be filtered by their type's package, package::module, or their fully qualified
    /// type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g. `0x2::coin::Coin`, or by
    /// the full type name, such as `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<String>,

    /// Filter for live objects by their current owners.
    pub owner: Option<SuiAddress>,

    /// Filter for live objects by their IDs.
    pub object_ids: Option<Vec<SuiAddress>>,
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

pub(crate) type Cursor = cursor::BcsCursor<Vec<u8>>;
type Query<ST, GB> = data::Query<ST, objects::table, GB>;

/// An object in Sui is a package (set of Move bytecode modules) or object (typed data structure
/// with fields) with additional metadata detailing its id, version, transaction digest, owner
/// field indicating how this object can be accessed.
#[Object]
impl Object {
    async fn version(&self) -> u64 {
        self.native.version().value()
    }

    /// 32-byte hash that identifies the object's current contents, encoded as a Base58 string.
    async fn digest(&self) -> String {
        if let Some(stored) = &self.stored {
            Base58::encode(&stored.object_digest)
        } else {
            self.native.digest().base58_encode()
        }
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated.
    /// This number is recalculated based on the present storage gas price.
    async fn storage_rebate(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.storage_rebate))
    }

    /// The set of named templates defined on-chain for the type of this object,
    /// to be handled off-chain. The server substitutes data from the object
    /// into these templates to generate a display string per template.
    async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        let resolver: &Resolver<PackageCache> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;
        let move_object = self
            .native
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
        if let Some(stored) = &self.stored {
            Ok(Some(Base64::from(&stored.serialized_object)))
        } else {
            let bytes = bcs::to_bytes(&self.native)
                .map_err(|e| {
                    Error::Internal(format!(
                        "Failed to serialize object at {}: {e}",
                        self.address,
                    ))
                })
                .extend()?;

            Ok(Some(Base64::from(&bytes)))
        }
    }

    /// The transaction block that created this version of the object.
    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        let digest = self.native.previous_transaction;
        TransactionBlock::query(ctx.data_unchecked(), digest.into())
            .await
            .extend()
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    /// Immutable and Shared Objects do not have owners.
    async fn owner(&self, ctx: &Context<'_>) -> Option<ObjectOwner> {
        use NativeOwner as O;

        match self.native.owner {
            O::AddressOwner(address) => {
                let address = SuiAddress::from(address);
                Some(ObjectOwner::Address(AddressOwner {
                    owner: Some(Owner { address }),
                }))
            }
            O::Immutable => Some(ObjectOwner::Immutable(Immutable { dummy: None })),
            O::ObjectOwner(address) => {
                let parent = Object::query(ctx.data_unchecked(), address.into(), None)
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

        Object::paginate(ctx.data_unchecked(), page, None, filter)
            .await
            .extend()
    }

    /// The balance of coin objects of a particular coin type owned by the object.
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

    /// The balances of all coin types owned by the object. Coins of the same type are grouped
    /// together into one Balance.
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

    /// The SuinsRegistration NFTs owned by the given object. These grant the owner
    /// the capability to manage the associated domain.
    pub async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, SuinsRegistration>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_suins_registrations(
                first,
                after,
                last,
                before,
                ctx.data_unchecked::<NameServiceConfig>(),
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
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicField)
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
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicObject)
            .await
            .extend()
    }

    /// The dynamic fields on an object.
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
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

impl Object {
    /// Construct a GraphQL object from a native object, without its stored (indexed) counterpart.
    pub(crate) fn from_native(address: SuiAddress, native: NativeObject) -> Object {
        Object {
            address,
            stored: None,
            native,
        }
    }

    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use objects::dsl;

        let address = address.into_vec();
        let version = version.map(|v| v as i64);

        let stored_obj: Option<StoredObject> = db
            .execute(move |conn| {
                conn.first(move || {
                    let mut query = dsl::objects
                        .filter(dsl::object_id.eq(address.clone()))
                        .limit(1)
                        .into_boxed();

                    // TODO: leverage objects_history
                    if let Some(version) = version {
                        query = query.filter(dsl::object_version.eq(version));
                    }

                    query
                })
                .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch object: {e}")))?;

        stored_obj.map(Self::try_from).transpose()
    }

    /// Query the database for a `page` of objects. The page uses the bytes of an Object ID as the
    /// cursor, and can optionally be further `filter`-ed. The `owner_type` is an optional
    /// additional filter, to constrain the objects to be those whose owner is of a particular kind
    /// (address-owned, object-owned, shared, immutable). This kind of filter is not exposed
    /// directly in the GraphQL API, but we can take advantage of it when constructing DB queries to
    /// serve certain other queries (e.g. dynamic field queries).
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        owner_type: Option<OwnerType>,
        filter: ObjectFilter,
    ) -> Result<Connection<String, Object>, Error> {
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

                    if let Some(owner_type) = &owner_type {
                        query = query.filter(dsl::owner_type.eq(*owner_type as i16));
                    }

                    if let Some(type_) = &filter.type_ {
                        query = query.filter(dsl::object_type.is_not_null());
                        query = type_.apply(query, dsl::object_type.assume_not_null());
                    }

                    if let Some(owner) = &filter.owner {
                        query = query.filter(dsl::owner_id.eq(owner.into_vec()));

                        // If we are supplying an address as the owner, we know that the object must
                        // be owned by an address, or by an object.
                        query = query.filter(
                            dsl::owner_type
                                .eq_any([OwnerType::Address as i16, OwnerType::Object as i16]),
                        );
                    }

                    query
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let object = Object::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, object));
        }

        Ok(conn)
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

impl Target<Cursor> for StoredObject {
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

    fn cursor(&self) -> Cursor {
        Cursor::new(self.object_id.clone())
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
            stored: Some(stored_object),
            native: native_object,
        })
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
