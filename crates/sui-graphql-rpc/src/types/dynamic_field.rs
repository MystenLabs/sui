// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use move_core_types::annotated_value::{self as A, MoveStruct};
use sui_indexer::models_v2::objects::StoredHistoryObject;
use sui_indexer::types_v2::OwnerType;
use sui_package_resolver::Resolver;
use sui_types::dynamic_field::{derive_dynamic_field_id, DynamicFieldInfo, DynamicFieldType};

use super::cursor::{Page, Target};
use super::object::{self, deserialize_move_struct, Object, ObjectKind, ObjectLookupKey};
use super::type_filter::ExactTypeFilter;
use super::{
    base64::Base64, move_object::MoveObject, move_value::MoveValue, sui_address::SuiAddress,
};
use crate::consistency::{build_objects_query, consistent_range, View};
use crate::context_data::package_cache::PackageCache;
use crate::data::{Db, QueryExecutor};
use crate::error::Error;
use crate::filter;
use crate::raw_query::RawQuery;

pub(crate) struct DynamicField {
    pub super_: MoveObject,
    pub df_object_id: SuiAddress,
    pub df_kind: DynamicFieldType,
}

#[derive(Union)]
pub(crate) enum DynamicFieldValue {
    MoveObject(MoveObject), // DynamicObject
    MoveValue(MoveValue),   // DynamicField
}

#[derive(InputObject)] // used as input object
pub(crate) struct DynamicFieldName {
    /// The string type of the DynamicField's 'name' field.
    /// A string representation of a Move primitive like 'u64', or a struct type like '0x2::kiosk::Listing'
    pub type_: ExactTypeFilter,
    /// The Base64 encoded bcs serialization of the DynamicField's 'name' field.
    pub bcs: Base64,
}

/// Dynamic fields are heterogeneous fields that can be added or removed at runtime,
/// and can have arbitrary user-assigned names. There are two sub-types of dynamic
/// fields:
///
/// 1) Dynamic Fields can store any value that has the `store` ability, however an object
///    stored in this kind of field will be considered wrapped and will not be accessible
///    directly via its ID by external tools (explorers, wallets, etc) accessing storage.
/// 2) Dynamic Object Fields values must be Sui objects (have the `key` and `store`
///    abilities, and id: UID as the first field), but will still be directly accessible off-chain
///    via their object ID after being attached.
#[Object]
impl DynamicField {
    /// The string type, data, and serialized value of the DynamicField's 'name' field.
    /// This field is used to uniquely identify a child of the parent object.
    async fn name(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>> {
        let resolver: &Resolver<PackageCache> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        let (struct_tag, move_struct) = deserialize_move_struct(&self.super_.native, resolver)
            .await
            .extend()?;

        // Get TypeTag of the DynamicField name from StructTag of the MoveStruct
        let type_tag = DynamicFieldInfo::try_extract_field_name(&struct_tag, &self.df_kind)
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;

        let name_move_value = extract_field_from_move_struct(move_struct, "name").extend()?;

        let undecorated = if self.df_kind == DynamicFieldType::DynamicObject {
            let inner_name_move_value = match name_move_value {
                A::MoveValue::Struct(inner_struct) => {
                    extract_field_from_move_struct(inner_struct, "name")
                }
                _ => Err(Error::Internal("Expected a wrapper struct".to_string())),
            }
            .extend()?;
            inner_name_move_value.undecorate()
        } else {
            name_move_value.undecorate()
        };

        let bcs = bcs::to_bytes(&undecorated)
            .map_err(|e| Error::Internal(format!("Failed to serialize object: {e}")))
            .extend()?;

        Ok(Some(MoveValue::new(type_tag, Base64::from(bcs))))
    }

    /// The actual data stored in the dynamic field.
    /// The returned dynamic field is an object if its return type is MoveObject,
    /// in which case it is also accessible off-chain via its address.
    async fn value(&self, ctx: &Context<'_>) -> Result<Option<DynamicFieldValue>> {
        if self.df_kind == DynamicFieldType::DynamicObject {
            let obj = MoveObject::query(
                ctx.data_unchecked(),
                self.df_object_id,
                match self.super_.super_.checkpoint_viewed_at {
                    Some(seq) => ObjectLookupKey::LatestAt(seq),
                    None => ObjectLookupKey::Latest,
                },
            )
            .await
            .extend()?;
            Ok(obj.map(DynamicFieldValue::MoveObject))
        } else {
            let resolver: &Resolver<PackageCache> = ctx
                .data()
                .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
                .extend()?;

            let (struct_tag, move_struct) = deserialize_move_struct(&self.super_.native, resolver)
                .await
                .extend()?;

            // Get TypeTag of the DynamicField value from StructTag of the MoveStruct
            let type_tag = DynamicFieldInfo::try_extract_field_value(&struct_tag)
                .map_err(|e| Error::Internal(e.to_string()))
                .extend()?;

            let value_move_value = extract_field_from_move_struct(move_struct, "value").extend()?;

            let undecorated = value_move_value.undecorate();
            let bcs = bcs::to_bytes(&undecorated)
                .map_err(|e| Error::Internal(format!("Failed to serialize object: {e}")))
                .extend()?;

            Ok(Some(DynamicFieldValue::MoveValue(MoveValue::new(
                type_tag,
                Base64::from(bcs),
            ))))
        }
    }
}

impl DynamicField {
    /// Fetch a single dynamic field entry from the `db`, on `parent` object, with field name
    /// `name`, and kind `kind` (dynamic field or dynamic object field).
    pub(crate) async fn query(
        db: &Db,
        parent: SuiAddress,
        name: DynamicFieldName,
        kind: DynamicFieldType,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<DynamicField>, Error> {
        let type_ = match kind {
            DynamicFieldType::DynamicField => name.type_.0,
            DynamicFieldType::DynamicObject => {
                DynamicFieldInfo::dynamic_object_field_wrapper(name.type_.0).into()
            }
        };

        let field_id = derive_dynamic_field_id(parent, &type_, &name.bcs.0)
            .map_err(|e| Error::Internal(format!("Failed to derive dynamic field id: {e}")))?;

        let super_ = MoveObject::query(
            db,
            SuiAddress::from(field_id),
            match checkpoint_viewed_at {
                Some(seq) => ObjectLookupKey::LatestAt(seq),
                None => ObjectLookupKey::Latest,
            },
        )
        .await?;

        super_.map(Self::try_from).transpose()
    }

    /// Query the `db` for a `page` of dynamic fields attached to object with ID `parent`.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<object::Cursor>,
        parent: SuiAddress,
        version: Option<u64>,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Connection<String, DynamicField>, Error> {
        // If cursors are provided, defer to the `checkpoint_viewed_at` in the cursor if they are
        // consistent. Otherwise, use the value from the parameter, or set to None. This is so that
        // paginated queries are consistent with the previous query that created the cursor.
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at: Option<u64> = cursor_viewed_at.or(checkpoint_viewed_at);

        let Some(((prev, next, results), checkpoint_viewed_at)) = db
            .execute_repeatable(move |conn| {
                let Some((lhs, rhs)) = consistent_range(conn, checkpoint_viewed_at)? else {
                    return Ok::<_, diesel::result::Error>(None);
                };

                let result = page.paginate_raw_query::<StoredHistoryObject>(
                    conn,
                    rhs,
                    dynamic_fields_query(parent, version, lhs as i64, rhs as i64),
                )?;

                Ok(Some((result, rhs)))
            })
            .await?
        else {
            return Err(Error::Client(
                "Requested data is outside the available range".to_string(),
            ));
        };

        let mut conn: Connection<String, DynamicField> = Connection::new(prev, next);

        for stored in results {
            // To maintain consistency, the returned cursor should have the same upper-bound as the
            // checkpoint found on the cursor.
            let cursor = stored
                .consistent_cursor(checkpoint_viewed_at)
                .encode_cursor();

            let object =
                Object::try_from_stored_history_object(stored, Some(checkpoint_viewed_at))?;

            let move_ = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Failed to deserialize as Move object: {}",
                    object.address
                ))
            })?;

            let dynamic_field = DynamicField::try_from(move_)?;

            conn.edges.push(Edge::new(cursor, dynamic_field));
        }

        Ok(conn)
    }
}

impl TryFrom<MoveObject> for DynamicField {
    type Error = Error;

    fn try_from(stored: MoveObject) -> Result<Self, Error> {
        let super_ = &stored.super_;

        let (df_object_id, df_kind) = match &super_.kind {
            ObjectKind::Live(_, stored) => stored
                .df_object_id
                .as_ref()
                .map(|id| (id, stored.df_kind))
                .ok_or_else(|| Error::Internal("Object is not a dynamic field.".to_string()))?,
            ObjectKind::Historical(_, stored) => stored
                .df_object_id
                .as_ref()
                .map(|id| (id, stored.df_kind))
                .ok_or_else(|| Error::Internal("Object is not a dynamic field.".to_string()))?,
            _ => {
                return Err(Error::Internal(
                    "A WrappedOrDeleted object cannot be converted into a DynamicField."
                        .to_string(),
                ))
            }
        };

        let df_object_id = SuiAddress::from_bytes(df_object_id).map_err(|e| {
            Error::Internal(format!("Failed to deserialize dynamic field ID: {e}."))
        })?;

        let df_kind = match df_kind {
            Some(0) => DynamicFieldType::DynamicField,
            Some(1) => DynamicFieldType::DynamicObject,
            Some(k) => {
                return Err(Error::Internal(format!(
                    "Unrecognized dynamic field kind: {k}."
                )))
            }
            None => return Err(Error::Internal("No dynamic field kind.".to_string())),
        };

        Ok(DynamicField {
            super_: stored,
            df_object_id,
            df_kind,
        })
    }
}

pub fn extract_field_from_move_struct(
    move_struct: MoveStruct,
    field_name: &str,
) -> Result<A::MoveValue, Error> {
    move_struct
        .fields
        .into_iter()
        .find_map(|(id, value)| {
            if id.to_string() == field_name {
                Some(value)
            } else {
                None
            }
        })
        .ok_or_else(|| Error::Internal(format!("Field '{}' not found", field_name)))
}

fn dynamic_fields_query(parent: SuiAddress, version: Option<u64>, lhs: i64, rhs: i64) -> RawQuery {
    let view = match version {
        Some(_) => View::Historical,
        None => View::Consistent,
    };
    build_objects_query(view, lhs, rhs, move |query| {
        apply_filter(query, parent, version)
    })
}

fn apply_filter(query: RawQuery, parent: SuiAddress, parent_version: Option<u64>) -> RawQuery {
    let query = filter!(
        query,
        format!(
            "owner_id = '\\x{}'::bytea AND owner_type = {} AND df_kind IS NOT NULL",
            hex::encode(parent.into_vec()),
            OwnerType::Object as i16
        )
    );

    if let Some(version) = parent_version {
        filter!(query, format!("object_version <= {}", version))
    } else {
        query
    }
}
