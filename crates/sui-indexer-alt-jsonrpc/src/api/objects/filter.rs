// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::{
    dsl::sql,
    sql_types::{BigInt, Bool, Bytea},
    BoolExpressionMethods, ExpressionMethods, JoinOnDsl, QueryDsl,
};
use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_indexer_alt_schema::{objects::StoredOwnerKind, schema::obj_info};
use sui_json_rpc_types::{Page as PageResponse, SuiObjectDataOptions};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    sui_serde::SuiStructTag,
    Identifier, TypeTag,
};

use crate::{
    error::RpcError,
    paginate::{BcsCursor, Cursor as _, Page},
    Context,
};

use super::{error::Error, ObjectsConfig};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", rename = "ObjectResponseQuery", default)]
pub(crate) struct SuiObjectResponseQuery {
    /// If None, no filter will be applied
    pub filter: Option<SuiObjectDataFilter>,
    /// config which fields to include in the response, by default only digest is included
    pub options: Option<SuiObjectDataOptions>,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) enum SuiObjectDataFilter {
    /// Query by the object type's package.
    Package(ObjectID),
    /// Query by the object type's module.
    MoveModule {
        /// The package that contains the module.
        package: ObjectID,
        /// The module name.
        #[schemars(with = "String")]
        module: Identifier,
    },
    /// Query by the object's type.
    StructType(
        #[serde_as(as = "SuiStructTag")]
        #[schemars(with = "String")]
        StructTag,
    ),
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectCursor {
    object_id: Vec<u8>,
    cp_sequence_number: u64,
}

type Cursor = BcsCursor<ObjectCursor>;
type ObjectIDs = PageResponse<ObjectID, String>;

impl SuiObjectDataFilter {
    fn package(&self) -> ObjectID {
        match self {
            SuiObjectDataFilter::Package(p) => *p,
            SuiObjectDataFilter::MoveModule { package, .. } => *package,
            SuiObjectDataFilter::StructType(tag) => tag.address.into(),
        }
    }

    fn module(&self) -> Option<&str> {
        match self {
            SuiObjectDataFilter::Package(_) => None,
            SuiObjectDataFilter::MoveModule { module, .. } => Some(module.as_str()),
            SuiObjectDataFilter::StructType(tag) => Some(tag.module.as_str()),
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            SuiObjectDataFilter::Package(_) => None,
            SuiObjectDataFilter::MoveModule { .. } => None,
            SuiObjectDataFilter::StructType(tag) => Some(tag.name.as_str()),
        }
    }

    fn type_params(&self) -> Option<&[TypeTag]> {
        match self {
            SuiObjectDataFilter::Package(_) => None,
            SuiObjectDataFilter::MoveModule { .. } => None,
            SuiObjectDataFilter::StructType(tag) => {
                (!tag.type_params.is_empty()).then(|| &tag.type_params[..])
            }
        }
    }
}

/// Fetch ObjectIDs for a page of objects owned by `owner` that satisfy the given `filter` and
/// pagination parameters. Returns the digests and a cursor point to the last result (if there are
/// any results).
pub(super) async fn owned_objects(
    ctx: &Context,
    config: &ObjectsConfig,
    owner: SuiAddress,
    filter: &Option<SuiObjectDataFilter>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<ObjectIDs, RpcError<Error>> {
    use obj_info::dsl as o;

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

    let page: Page<Cursor> = Page::from_params(
        config.default_page_size,
        config.max_page_size,
        cursor,
        limit,
        None,
    )?;

    let mut query = candidates
        .select(candidates!(object_id, cp_sequence_number))
        .left_join(
            newer.on(candidates!(object_id)
                .eq(newer!(object_id))
                .and(candidates!(cp_sequence_number).lt(newer!(cp_sequence_number)))),
        )
        .filter(newer!(object_id).is_null())
        .filter(candidates!(owner_kind).eq(StoredOwnerKind::Address))
        .filter(candidates!(owner_id).eq(owner.to_inner()))
        .order_by(candidates!(cp_sequence_number).desc())
        .then_order_by(candidates!(object_id).desc())
        .limit(page.limit + 1)
        .into_boxed();

    if let Some(c) = page.cursor {
        query = query.filter(
            sql::<Bool>(r#"("candidates"."cp_sequence_number", "candidates"."object_id") < ("#)
                .bind::<BigInt, _>(c.cp_sequence_number as i64)
                .sql(", ")
                .bind::<Bytea, _>(c.object_id.clone())
                .sql(")"),
        );
    }

    let filter = filter.as_ref();
    if let Some(package) = filter.map(|f| f.package()) {
        query = query.filter(candidates!(package).eq(package.into_bytes()));
    }

    if let Some(module) = filter.and_then(|f| f.module()) {
        query = query.filter(candidates!(module).eq(module));
    }

    if let Some(name) = filter.and_then(|f| f.name()) {
        query = query.filter(candidates!(name).eq(name));
    }

    if let Some(type_params) = filter.and_then(|f| f.type_params()) {
        let bytes = bcs::to_bytes(type_params).context("Failed to serialize type params")?;
        query = query.filter(candidates!(instantiation).eq(bytes));
    }

    let mut results: Vec<(Vec<u8>, i64)> = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(query)
        .await
        .context("Failed to fetch object info")?;

    let has_next_page = results.len() > page.limit as usize;
    if has_next_page {
        results.truncate(page.limit as usize);
    }

    let next_cursor = results
        .last()
        .map(|(o, c)| {
            BcsCursor(ObjectCursor {
                object_id: o.clone(),
                cp_sequence_number: *c as u64,
            })
            .encode()
        })
        .transpose()
        .context("Failed to encode next cursor")?;

    let data: Vec<ObjectID> = results
        .into_iter()
        .map(|(o, _)| ObjectID::from_bytes(o))
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to deserialize Object IDs")?;

    Ok(PageResponse {
        data,
        next_cursor,
        has_next_page,
    })
}
