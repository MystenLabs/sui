// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use move_core_types::annotated_value::{self as A, MoveStruct};
use sui_indexer::models_v2::objects::StoredObject;
use sui_indexer::schema_v2::objects;
use sui_indexer::types_v2::OwnerType;
use sui_package_resolver::Resolver;
use sui_types::dynamic_field::{derive_dynamic_field_id, DynamicFieldInfo, DynamicFieldType};

use super::cursor::{Page, Target};
use super::object::{self, deserialize_move_struct};
use super::type_filter::ExactTypeFilter;
use super::{
    base64::Base64, move_object::MoveObject, move_value::MoveValue, sui_address::SuiAddress,
};
use crate::context_data::package_cache::PackageCache;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use sui_types::object::Object as NativeObject;

pub(crate) struct DynamicField {
    pub stored: StoredObject,
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
        let native_object: NativeObject = bcs::from_bytes(&self.stored.serialized_object)
            .map_err(|e| Error::Internal(format!("Failed to deserialize object: {e}")))?;
        let move_object = native_object
            .data
            .try_as_move()
            .ok_or_else(|| Error::Internal("Failed to convert object into MoveObject".to_string()))
            .extend()?;

        let (struct_tag, move_struct) = deserialize_move_struct(move_object, resolver)
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
            let obj = MoveObject::query(ctx.data_unchecked(), self.df_object_id, None)
                .await
                .extend()?;
            Ok(obj.map(DynamicFieldValue::MoveObject))
        } else {
            let resolver: &Resolver<PackageCache> = ctx
                .data()
                .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
                .extend()?;
            let native_object: NativeObject = bcs::from_bytes(&self.stored.serialized_object)
                .map_err(|e| Error::Internal(format!("Failed to deserialize object: {e}")))?;
            let move_object = native_object
                .data
                .try_as_move()
                .ok_or_else(|| {
                    Error::Internal("Failed to convert object into MoveObject".to_string())
                })
                .extend()?;

            let (struct_tag, move_struct) = deserialize_move_struct(move_object, resolver)
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
    ) -> Result<Option<DynamicField>, Error> {
        use objects::dsl;

        let type_ = match kind {
            DynamicFieldType::DynamicField => name.type_.0,
            DynamicFieldType::DynamicObject => {
                DynamicFieldInfo::dynamic_object_field_wrapper(name.type_.0).into()
            }
        };

        let field_id = derive_dynamic_field_id(parent, &type_, &name.bcs.0)
            .map_err(|e| Error::Internal(format!("Failed to derive dynamic field id: {e}")))?
            .to_vec();

        let stored: Option<StoredObject> = db
            .execute(move |conn| {
                conn.first(move || dsl::objects.filter(dsl::object_id.eq(field_id.clone())))
                    .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch object: {e}")))?;

        stored.map(Self::try_from).transpose()
    }

    /// Query the `db` for a `page` of dynamic fields attached to object with ID `parent`.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<object::Cursor>,
        parent: SuiAddress,
    ) -> Result<Connection<String, DynamicField>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredObject, _, _, _>(conn, move || {
                    use objects::dsl;
                    dsl::objects
                        .filter(dsl::owner_id.eq(parent.into_vec()))
                        .filter(dsl::owner_type.eq(OwnerType::Object as i16))
                        .filter(dsl::df_kind.is_not_null())
                        .into_boxed()
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = stored.cursor().encode_cursor();
            let field = DynamicField::try_from(stored)?;
            conn.edges.push(Edge::new(cursor, field));
        }

        Ok(conn)
    }
}

impl TryFrom<StoredObject> for DynamicField {
    type Error = Error;

    fn try_from(stored: StoredObject) -> Result<Self, Error> {
        let Some(df_object_id) = stored.df_object_id.as_ref() else {
            return Err(Error::Internal(
                "Object is not a dynamic field.".to_string(),
            ));
        };

        let df_object_id = SuiAddress::from_bytes(df_object_id).map_err(|e| {
            Error::Internal(format!("Failed to deserialize dynamic field ID: {e}."))
        })?;

        let df_kind = match stored.df_kind {
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
            stored,
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
