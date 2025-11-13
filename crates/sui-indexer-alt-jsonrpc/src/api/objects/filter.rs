// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::{BoolExpressionMethods, ExpressionMethods, JoinOnDsl, QueryDsl, sql_types::Bool};
use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_indexer_alt_schema::{
    objects::{StoredObjInfo, StoredOwnerKind},
    schema::obj_info,
};
use sui_json_rpc_types::{Page as PageResponse, SuiObjectDataOptions};
use sui_sql_macro::sql;
use sui_types::{
    Identifier, SUI_FRAMEWORK_ADDRESS, TypeTag,
    base_types::{ObjectID, SuiAddress},
    dynamic_field::{DYNAMIC_FIELD_FIELD_STRUCT_NAME, DYNAMIC_FIELD_MODULE_NAME},
    sui_serde::SuiStructTag,
};

use crate::{
    context::Context,
    error::{RpcError, invalid_params},
    paginate::{BcsCursor, Cursor as _, Page},
};

use super::error::Error;

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
    /// Query for object's that don't match any of these filters.
    MatchNone(Vec<SuiObjectDataFilter>),

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

/// [SuiObjectDataFilter] converted into fields that can be compared directly with values coming
/// from the database.
enum RawFilter {
    MatchNone(Vec<RawFilter>),
    Type {
        package: Vec<u8>,
        module: Option<String>,
        name: Option<String>,
        instantiation: Option<Vec<u8>>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ObjectCursor {
    object_id: Vec<u8>,
    cp_sequence_number: u64,
}

pub(crate) type Cursor = BcsCursor<ObjectCursor>;
pub(crate) type ObjectIDs = PageResponse<ObjectID, String>;

impl SuiObjectDataFilter {
    /// Whether this is a compound filter (which is implemented using sequential scan), or a simple
    /// type filter, which can leverage indices on the database.
    fn is_compound(&self) -> bool {
        use SuiObjectDataFilter as F;
        matches!(self, F::MatchNone(_))
    }

    fn package(&self) -> Option<ObjectID> {
        use SuiObjectDataFilter as F;
        match self {
            F::MatchNone(_) => None,
            F::Package(p) => Some(*p),
            F::MoveModule { package, .. } => Some(*package),
            F::StructType(tag) => Some(tag.address.into()),
        }
    }

    fn module(&self) -> Option<&str> {
        use SuiObjectDataFilter as F;
        match self {
            F::MatchNone(_) => None,
            F::Package(_) => None,
            F::MoveModule { module, .. } => Some(module.as_str()),
            F::StructType(tag) => Some(tag.module.as_str()),
        }
    }

    fn name(&self) -> Option<&str> {
        use SuiObjectDataFilter as F;
        match self {
            F::MatchNone(_) => None,
            F::Package(_) => None,
            F::MoveModule { .. } => None,
            F::StructType(tag) => Some(tag.name.as_str()),
        }
    }

    fn type_params(&self) -> Option<&[TypeTag]> {
        use SuiObjectDataFilter as F;
        match self {
            F::MatchNone(_) => None,
            F::Package(_) => None,
            F::MoveModule { .. } => None,
            F::StructType(tag) => (!tag.type_params.is_empty()).then(|| &tag.type_params[..]),
        }
    }

    /// Convert this filter into a raw filter which can be matched against a row from the database.
    /// This operation can fail if the filter exceeds limits (too many type filters or too deep).
    fn to_raw(&self, ctx: &Context) -> Result<RawFilter, RpcError<Error>> {
        let config = &ctx.config().objects;

        fn convert(
            filter: &SuiObjectDataFilter,
            max_depth: usize,
            mut depth: usize,
            max_type_filters: usize,
            type_filters: &mut usize,
        ) -> Result<RawFilter, RpcError<Error>> {
            if depth == 0 {
                return Err(invalid_params(Error::FilterTooDeep { max: max_depth }));
            } else {
                depth -= 1;
            }

            if !matches!(filter, F::MatchNone(_)) {
                if *type_filters == 0 {
                    return Err(invalid_params(Error::FilterTooBig {
                        max: max_type_filters,
                    }));
                } else {
                    *type_filters -= 1;
                }
            }

            use RawFilter as R;
            use SuiObjectDataFilter as F;
            Ok(match filter {
                F::MatchNone(filters) => R::MatchNone(
                    filters
                        .iter()
                        .map(|f| convert(f, max_depth, depth, max_type_filters, type_filters))
                        .collect::<Result<Vec<_>, _>>()?,
                ),

                F::Package(object_id) => R::Type {
                    package: object_id.to_vec(),
                    module: None,
                    name: None,
                    instantiation: None,
                },

                F::MoveModule { package, module } => R::Type {
                    package: package.to_vec(),
                    module: Some(module.to_string()),
                    name: None,
                    instantiation: None,
                },

                F::StructType(struct_tag) => R::Type {
                    package: struct_tag.address.to_vec(),
                    module: Some(struct_tag.module.to_string()),
                    name: Some(struct_tag.name.to_string()),
                    instantiation: (!struct_tag.type_params.is_empty())
                        .then(|| bcs::to_bytes(&struct_tag.type_params))
                        .transpose()
                        .context("Failed to serialize type parameters in filter")?,
                },
            })
        }

        let depth = config.max_filter_depth;
        let mut type_filters = config.max_type_filters;
        convert(self, depth, depth, type_filters, &mut type_filters)
    }
}

impl RawFilter {
    /// Check whether the given `info` from the database matches this filter.
    fn matches(&self, info: &StoredObjInfo) -> bool {
        let (Some(package), Some(module), Some(name), Some(instantiation)) =
            (&info.package, &info.module, &info.name, &info.instantiation)
        else {
            // If any of these fields are `None`, the record is for a deleted object which cannot
            // match any filters.
            return false;
        };

        use RawFilter as R;
        let (package_filter, module_filter, name_filter, instantiation_filter) = match self {
            R::MatchNone(raw_filters) => return !raw_filters.iter().any(|f| f.matches(info)),

            R::Type {
                package,
                module,
                name,
                instantiation,
            } => (
                package,
                module.as_ref(),
                name.as_ref(),
                instantiation.as_ref(),
            ),
        };

        if package_filter != package {
            return false;
        }

        if module_filter.is_some_and(|m| m != module) {
            return false;
        }

        if name_filter.is_some_and(|n| n != name) {
            return false;
        }

        if instantiation_filter.is_some_and(|i| i != instantiation) {
            return false;
        }

        true
    }
}

/// Fetch ObjectIDs for a page of objects owned by `owner` that satisfy the given `filter` and
/// pagination parameters. Returns the IDs and a cursor pointing to the last result (if there are
/// any results).
pub(super) async fn owned_objects(
    ctx: &Context,
    owner: SuiAddress,
    filter: &Option<SuiObjectDataFilter>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<ObjectIDs, RpcError<Error>> {
    match filter {
        Some(f) if f.is_compound() => {
            by_sequential_scan(ctx, owner, &f.to_raw(ctx)?, cursor, limit).await
        }
        filter => {
            by_type_indices(ctx, owner, StoredOwnerKind::Address, filter, cursor, limit).await
        }
    }
}

/// Fetch ObjectIDs for a page of dynamic fields owned by parent object `owner`. The returned IDs
/// all point to `sui::dynamic_field::Field<K, V>` objects. Returns the IDs and a cursor pointing
/// to the last result (if there are any results).
pub(crate) async fn dynamic_fields(
    ctx: &Context,
    owner: ObjectID,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<ObjectIDs, RpcError<Error>> {
    by_type_indices(
        ctx,
        owner.into(),
        StoredOwnerKind::Object,
        &Some(SuiObjectDataFilter::StructType(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: DYNAMIC_FIELD_MODULE_NAME.to_owned(),
            name: DYNAMIC_FIELD_FIELD_STRUCT_NAME.to_owned(),
            type_params: vec![],
        })),
        cursor,
        limit,
    )
    .await
}

/// Fetch ObjectIDs for a page of objects owned by `owner` that satisfy the given compound
/// `filter`. Works by repeatedly fetching pages of objects owned by the owner, filtering out only
/// matching entries until the limit is met.
async fn by_sequential_scan(
    ctx: &Context,
    owner: SuiAddress,
    filter: &RawFilter,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<ObjectIDs, RpcError<Error>> {
    let config = &ctx.config().objects;

    let page: Page<Cursor> = Page::from_params(
        config.default_page_size,
        config.max_page_size,
        cursor,
        limit,
        None,
    )?;

    // Initially, be optimistic and assume that all the results fetched will match.
    let mut results = vec![];
    let mut cursor = page.cursor.map(|c| c.0);
    let mut fetch = page.limit + 1;
    let mut scans = 0;

    loop {
        let infos = owned_obj_info(ctx, owner, &cursor, fetch).await?;

        for info in &infos {
            if filter.matches(info) {
                results.push((info.object_id.clone(), info.cp_sequence_number as u64));
            }
        }

        // If there isn't a last object, we can't compute a next cursor -- stop fetching more info
        // rows.
        let Some(last) = infos.last() else {
            break;
        };

        // If we have enough data to satisfy the filtered page, or we got back less data than we
        // asked for in the last request, stop. Otherwise fetch more owned objects from where we
        // left off, in larger chunks.
        if results.len() > page.limit as usize || infos.len() < fetch as usize {
            break;
        }

        scans += 1;
        fetch = config.filter_scan_size as i64;
        cursor = Some(ObjectCursor {
            object_id: last.object_id.clone(),
            cp_sequence_number: last.cp_sequence_number as u64,
        });
    }

    ctx.metrics()
        .owned_objects_filter_scans
        .observe(scans as f64);

    let has_next_page = results.len() > page.limit as usize;
    if has_next_page {
        results.truncate(page.limit as usize)
    }

    // We cannot re-use `cursor` from the loop above here, because we may have over-fetched and
    // then discarded results when calculating whether we have a next page (above).
    let next_cursor = results
        .last()
        .map(|(o, c)| {
            BcsCursor(ObjectCursor {
                object_id: o.clone(),
                cp_sequence_number: *c,
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
        has_next_page,
        next_cursor,
    })
}

/// Fetch a page of `StoredObjInfo` corresponding to objects owned by `owner`.
async fn owned_obj_info(
    ctx: &Context,
    owner: SuiAddress,
    cursor: &Option<ObjectCursor>,
    limit: i64,
) -> Result<Vec<StoredObjInfo>, RpcError<Error>> {
    use obj_info::dsl as o;

    let (candidates, newer) = diesel::alias!(obj_info as candidates, obj_info as newer);

    macro_rules! candidates {
        ($($field:ident),* $(,)?) => {
            candidates.fields(($(o::$field),*))
        };
    }

    macro_rules! newer {
        ($($field:ident),* $(,)?) => {
            newer.fields(($(o::$field),*))
        };
    }

    let mut query = candidates
        .select(candidates!(
            object_id,
            cp_sequence_number,
            owner_kind,
            owner_id,
            package,
            module,
            name,
            instantiation,
        ))
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
        .limit(limit)
        .into_boxed();

    if let Some(c) = cursor {
        query = query.filter(sql!(as Bool,
            "(candidates.cp_sequence_number, candidates.object_id) < ({BigInt}, {Bytea})",
            c.cp_sequence_number as i64,
            c.object_id.clone(),
        ));
    }

    Ok(ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(query)
        .await
        .context("Failed to fetch object info")?)
}

/// Fetch ObjectIDs for a page of objects owned by `owner` that satisfy the given `filter` which is
/// assumed to be a simple type filter that can be served by indices in the database, as well as
/// the pagination parameters. Returns the IDs and a cursor pointing to the last result (if
/// there are any results).
async fn by_type_indices(
    ctx: &Context,
    owner: SuiAddress,
    kind: StoredOwnerKind,
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

    let config = &ctx.config().objects;
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
        .filter(candidates!(owner_kind).eq(kind))
        .filter(candidates!(owner_id).eq(owner.to_inner()))
        .order_by(candidates!(cp_sequence_number).desc())
        .then_order_by(candidates!(object_id).desc())
        .limit(page.limit + 1)
        .into_boxed();

    if let Some(c) = page.cursor {
        query = query.filter(sql!(as Bool,
            "(candidates.cp_sequence_number, candidates.object_id) < ({BigInt}, {Bytea})",
            c.cp_sequence_number as i64,
            c.object_id.clone(),
        ));
    }

    let filter = filter.as_ref();
    if let Some(package) = filter.and_then(|f| f.package()) {
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
        .pg_reader()
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
