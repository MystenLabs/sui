// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::pg_reader::PgReader;
use crate::data::error::Error;
use diesel::prelude::*;
use diesel::{ExpressionMethods, QueryDsl};
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use sui_indexer_alt_schema::schema::obj_info;
use sui_types::base_types::ObjectID;

// TODO: potentially make this a data loader.
/// Load the object ID of the singleton object for the given struct tag.
/// Only the latest modified (created or transferred) object with the given type
/// is returned even if there might be multiple objects with the same type.
/// Returns None if no singleton object is found or if a row is found but the
/// object has been deleted.
pub(crate) async fn load_singleton_object_id(
    reader: &PgReader,
    struct_tag: StructTag,
) -> Result<Option<ObjectID>, Arc<Error>> {
    use obj_info::dsl as o;

    let package = struct_tag.address.to_vec();
    let module = struct_tag.module.as_str();
    let name = struct_tag.name.as_str();
    let instantiation =
        bcs::to_bytes(&struct_tag.type_params).map_err(|e| Arc::new(Error::Serde(e.into())))?;

    let (candidates, newer) = diesel::alias!(obj_info as candidates, obj_info as newer);

    macro_rules! candidates {
        ($($field:ident),*) => {
            candidates.fields(($(o::$field),*))
        };
    }

    macro_rules! newer {
        ($($field:ident),*) => {
            newer.fields(($(o::$field),*))
        };
    }

    let query = candidates
        .select(candidates!(object_id))
        .left_join(
            newer.on(candidates!(object_id)
                .eq(newer!(object_id))
                .and(candidates!(cp_sequence_number).lt(newer!(cp_sequence_number)))),
        )
        // Only consider the latest status of the object.
        .filter(newer!(object_id).is_null())
        // Only consider objects that are not deleted.
        .filter(candidates!(owner_kind).is_not_null())
        .filter(candidates!(package).eq(package))
        .filter(candidates!(module).eq(module))
        .filter(candidates!(name).eq(name))
        .filter(candidates!(instantiation).eq(instantiation))
        .order_by(candidates!(cp_sequence_number).desc());

    let mut conn = reader.connect().await.map_err(Arc::new)?;

    let stored: Option<Vec<u8>> = conn.first(query).await.map_err(Arc::new)?;
    stored
        .map(|b| ObjectID::from_bytes(&b))
        .transpose()
        .map_err(|e| Arc::new(Error::Serde(e.into())))
}
