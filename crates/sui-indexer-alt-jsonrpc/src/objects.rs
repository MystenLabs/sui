// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: Eventually move this module to a common crate so that both json-rpc and graphql can use.

// TODO: Remove all the dead_code annotations once we have a use for this file.

use diesel::dsl::sql_query;
use diesel::{
    sql_types::{BigInt, Bytea},
    QueryableByName, Selectable,
};
use diesel_async::RunQueryDsl;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_schema::objects::StoredOwnerKind;
use sui_indexer_alt_schema::schema::{obj_info, obj_versions};
use sui_pg_db::{Connection, Db};
use sui_types::base_types::SuiAddress;
use sui_types::TypeTag;

#[allow(dead_code)]
pub(crate) struct Cursor {
    // The cursor checkpoint number
    checkpoint: i64,
    // The cursor object ID
    object_id: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeFilter {
    /// Filter the object type by the package it's from.
    Package(SuiAddress),

    /// Filter the object type by the package and module it's from.
    Module(SuiAddress, String),

    /// Filter the object type by the package, module, name, and type parameters.
    /// If the struct tag has type parameters, treat it as an exact filter on that instantiation,
    /// otherwise treat it as either a filter on all generic instantiations of the type, or an exact
    /// match on the type with no type parameters. E.g.
    ///
    ///  0x2::coin::Coin
    ///
    /// would match both 0x2::coin::Coin and 0x2::coin::Coin<0x2::sui::SUI>.
    FullType(StructTag),
}

#[allow(dead_code)]
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectFilter {
    /// Filter objects by their type's `package`, `package::module`, or their fully qualified type
    /// name.
    ///
    /// Generic types can be queried by either the generic type name, e.g. `0x2::coin::Coin`, or by
    /// the full type name, such as `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_filter: Option<TypeFilter>,

    /// Filter for live objects by their current owners.
    pub owner_filter: Option<SuiAddress>,
}

#[allow(dead_code)]
#[derive(QueryableByName, Selectable, Debug)]
#[diesel(table_name = obj_versions)]
pub(crate) struct IdVersion {
    #[diesel(sql_type = Bytea)]
    object_id: Vec<u8>,
    #[diesel(sql_type = BigInt)]
    object_version: i64,
}

#[allow(dead_code)]
#[derive(Clone)]
enum CursorMode {
    // Continue within a specific checkpoint and query all objects within that checkpoint after the
    // cursor object ID.
    ContinueWithinCheckpoint(i64, Vec<u8>),
    // Continue after a specific checkpoint, i.e. query all objects whose checkpoint number is
    // bounded by the cursor checkpoint number.
    ContinueAfterCheckpoint(i64),
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct ObjectFilterQueryBuilder {
    package: Option<SuiAddress>,
    module: Option<String>,
    name: Option<String>,
    type_params: Option<Vec<TypeTag>>,
    owner: Option<SuiAddress>,
    view_checkpoint: Option<i64>,
}

#[allow(dead_code)]
#[derive(QueryableByName, Selectable, Debug)]
#[diesel(table_name = obj_info)]
struct IdCheckpoint {
    #[diesel(sql_type = BigInt)]
    cp_sequence_number: i64,
    #[diesel(sql_type = Bytea)]
    object_id: Vec<u8>,
}

#[allow(dead_code)]
impl ObjectFilterQueryBuilder {
    pub fn with_view_checkpoint(mut self, view_checkpoint: i64) -> Self {
        self.view_checkpoint = Some(view_checkpoint);
        self
    }

    pub fn with_type_filter(mut self, type_filter: TypeFilter) -> Self {
        match type_filter {
            TypeFilter::Package(package) => self.package = Some(package),
            TypeFilter::Module(package, module) => {
                self.package = Some(package);
                self.module = Some(module);
            }
            TypeFilter::FullType(struct_tag) => {
                self.package = Some(struct_tag.address.into());
                self.module = Some(struct_tag.module.as_str().to_owned());
                self.name = Some(struct_tag.name.as_str().to_owned());
                if !struct_tag.type_params.is_empty() {
                    // Not setting type_paramms when the struct tag has no type parameters, allowing us
                    // to query for all generic instantiations of the type.
                    self.type_params = Some(struct_tag.type_params);
                }
            }
        }
        self
    }

    pub fn with_owner_filter(mut self, owner: SuiAddress) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn build(self, limit: usize, cursor_mode: Option<CursorMode>) -> String {
        let Self {
            package,
            module,
            name,
            type_params,
            owner,
            view_checkpoint,
        } = self;
        let mut filter_conditions = vec![];
        if let Some(owner) = owner {
            filter_conditions.push(format!("owner_kind = {}", StoredOwnerKind::Address as i16));
            filter_conditions.push(format!(
                "owner_id = '\\x{}'::bytea",
                hex::encode(owner.to_vec())
            ));
        }
        if let Some(package) = package {
            filter_conditions.push(format!(
                "package = '\\x{}'::bytea",
                hex::encode(package.to_vec())
            ));
        }

        // FIXME: Adding module and name as strings is prone to SQL injection.
        // We will fix them when we move to a better way of building raw SQL queries.
        if let Some(module) = module {
            filter_conditions.push(format!("module = '{}'", module));
        }

        if let Some(name) = name {
            filter_conditions.push(format!("name = '{}'", name));
        }

        if let Some(type_params) = type_params {
            filter_conditions.push(format!(
                "instantiation = '\\x{}'::bytea",
                hex::encode(bcs::to_bytes(&type_params).unwrap())
            ));
        }

        if let Some(cursor_mode) = cursor_mode {
            match cursor_mode {
                CursorMode::ContinueWithinCheckpoint(checkpoint, object_id) => {
                    filter_conditions.push(format!("cp_sequence_number = {}", checkpoint));
                    filter_conditions.push(format!(
                        "object_id > '\\x{}'::bytea",
                        hex::encode(object_id)
                    ));
                }
                CursorMode::ContinueAfterCheckpoint(checkpoint) => {
                    filter_conditions.push(format!("cp_sequence_number < {}", checkpoint));
                }
            }
        } else if let Some(view_checkpoint) = view_checkpoint {
            // We only need to add the view checkpoint number condition if there is no cursor.
            // This is because the cursor implicitly includes a checkpoint number.
            filter_conditions.push(format!("cp_sequence_number <= {view_checkpoint}"));
        }

        let filter_conditions_str = filter_conditions.join(" AND ");
        let filtered_rows = format!(
            "SELECT
                cp_sequence_number,
                object_id
            FROM
                obj_info
            WHERE
                {filter_conditions_str}"
        );

        let mut join_conditions = vec![];
        join_conditions.push("o.object_id = f.object_id".to_string());
        join_conditions.push("o.cp_sequence_number > f.cp_sequence_number".to_string());
        if let Some(view_checkpoint) = view_checkpoint {
            join_conditions.push(format!("o.cp_sequence_number <= {view_checkpoint}"));
        }
        let join_conditions_str = join_conditions.join(" AND ");

        // This query queries all the objects that match the filter conditions,
        // but only return them if they are not updated in a later checkpoint bounded by the view
        // checkpoint number.
        format!(
            "
            WITH filtered_rows AS (
                {filtered_rows}
            )
            SELECT
                f.cp_sequence_number,
                f.object_id
            FROM
                filtered_rows f
            LEFT JOIN
                obj_info o
            ON
                {join_conditions_str}
            WHERE
                o.object_id IS NULL
            ORDER BY
                f.cp_sequence_number DESC,
                f.object_id ASC
            LIMIT {limit}
            ",
        )
    }
}

/// Given a set of filters, query the objects that match the filters.
/// Bound the query by an optional view checkpoint number, which is only required for consistent queries.
/// If cursor is provided, continue from the cursor.
/// Limit the number of objects returned.
/// Return the objects and the new cursor.
#[allow(dead_code)]
pub async fn query_objects_with_filters(
    db: &Db,
    filters: ObjectFilter,
    view_checkpoint: Option<i64>,
    cursor: Option<Cursor>,
    limit: usize,
) -> anyhow::Result<(Vec<IdVersion>, Option<Cursor>)> {
    if limit == 0 {
        return Ok((vec![], None));
    }
    let mut conn = db.connect().await?;
    let object_ids =
        query_object_ids_with_filters(&mut conn, filters, view_checkpoint, cursor, limit).await?;
    let next_cursor = if object_ids.len() == limit {
        // unwrap safe since limit is not 0, and hence object_ids.len() is not 0.
        let last = object_ids.last().unwrap();
        Some(Cursor {
            checkpoint: last.cp_sequence_number,
            object_id: last.object_id.clone(),
        })
    } else {
        None
    };
    // The obj_info table only tracks ownership or presence changes for objects.
    // To get the latest object versions that cover all mutations, we need to query the obj_versions table.
    let object_versions =
        query_latest_object_versions(&mut conn, &object_ids, view_checkpoint).await?;
    Ok((object_versions, next_cursor))
}

fn build_object_ids_query(
    filters: ObjectFilter,
    view_checkpoint: Option<i64>,
    cursor: Option<Cursor>,
    limit: usize,
) -> String {
    let mut builder = ObjectFilterQueryBuilder::default();
    if let Some(view_checkpoint) = view_checkpoint {
        builder = builder.with_view_checkpoint(view_checkpoint);
    }
    if let Some(type_filter) = filters.type_filter {
        builder = builder.with_type_filter(type_filter);
    }
    if let Some(owner_filter) = filters.owner_filter {
        builder = builder.with_owner_filter(owner_filter);
    }
    if let Some(cursor) = cursor {
        // Since Postgres cannot always generate an efficient query plan when there is a cursor,
        // we split the query into two parts.
        // The first part queries the objects that match the filters and are within the same checkpoint
        // as the cursor, but after the cursor object ID.
        // The second part queries the objects that match the filters and continue after the cursor
        // checkpoint number.
        // We execute them in the same query because most of the time, the first part returns less
        // than the limit, since within the same checkpoint, the number of objects that match the
        // filters is usually small.
        let query1 = builder.clone().build(
            limit,
            Some(CursorMode::ContinueWithinCheckpoint(
                cursor.checkpoint,
                cursor.object_id,
            )),
        );
        let query2 = builder.build(
            limit,
            Some(CursorMode::ContinueAfterCheckpoint(cursor.checkpoint)),
        );
        // Combine the two queries using UNION ALL, and limit the total number of objects to the limit.
        format!(
            "
            SELECT * FROM (
                ({query1})
                UNION ALL
                ({query2})
            ) AS combined
            LIMIT {limit}
            ",
        )
    } else {
        builder.build(limit, None)
    }
}

async fn query_object_ids_with_filters(
    conn: &mut Connection<'_>,
    filters: ObjectFilter,
    view_checkpoint: Option<i64>,
    cursor: Option<Cursor>,
    limit: usize,
) -> anyhow::Result<Vec<IdCheckpoint>> {
    let query = build_object_ids_query(filters, view_checkpoint, cursor, limit);
    Ok(sql_query(query).load::<IdCheckpoint>(conn).await?)
}

fn build_latest_object_versions_query<'a>(
    object_ids: impl IntoIterator<Item = &'a Vec<u8>>,
    view_checkpoint_number: Option<i64>,
) -> String {
    let sub_queries = object_ids
        .into_iter()
        .enumerate()
        .map(|(i, o)| {
            let mut filter_conditions = vec![];
            filter_conditions.push(format!("object_id = '\\x{}'::bytea", hex::encode(o)));
            if let Some(view_checkpoint_number) = view_checkpoint_number {
                filter_conditions.push(format!("cp_sequence_number <= {view_checkpoint_number}"));
            }
            let filter_conditions_str = filter_conditions.join(" AND ");
            format!(
                "SELECT * FROM (
                    SELECT object_id, object_version
                    FROM obj_versions
                    WHERE {filter_conditions_str}
                    ORDER BY cp_sequence_number DESC, object_version DESC
                    LIMIT 1
                ) AS subquery{i}",
            )
        })
        .collect::<Vec<_>>();
    sub_queries.join(" UNION ALL ")
}

async fn query_latest_object_versions(
    conn: &mut Connection<'_>,
    objects: &[IdCheckpoint],
    view_checkpoint_number: Option<i64>,
) -> anyhow::Result<Vec<IdVersion>> {
    if objects.is_empty() {
        return Ok(vec![]);
    }
    let query = build_latest_object_versions_query(
        objects.iter().map(|o| &o.object_id),
        view_checkpoint_number,
    );
    Ok(sql_query(query).load::<IdVersion>(conn).await?)
}

#[cfg(test)]
mod tests {
    use move_core_types::ident_str;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::MIGRATIONS;

    use super::*;

    #[derive(QueryableByName, Debug)]
    struct ExplainAnalyzeRow {
        #[diesel(sql_type = diesel::sql_types::Text)]
        #[diesel(column_name = "QUERY PLAN")]
        query_plan: String,
    }

    async fn get_query_plan(query: &str, conn: &mut Connection<'_>) -> String {
        let explain_query = format!("EXPLAIN ANALYZE {}", query);
        let results = sql_query(explain_query)
            .load::<ExplainAnalyzeRow>(conn)
            .await
            .unwrap();
        results
            .into_iter()
            .map(|r| r.query_plan)
            .collect::<Vec<_>>()
            .join("\n")
    }

    // This function splits the plan into steps using the `->` operator, and checks that it has the same
    // number of steps as the expected steps, and that each step contains the expected step from the
    // list of expected steps.
    fn assert_match_query_plan(expected_steps: &[&str], plan: &str) {
        let steps = plan.split("->").collect::<Vec<_>>();
        assert_eq!(
            steps.len(),
            expected_steps.len(),
            "Number of steps in plan does not match expected number of steps: {plan}"
        );
        for (i, step) in steps.iter().enumerate() {
            let expected_step = expected_steps[i];
            assert!(
                step.contains(expected_step),
                "Expected step {i} ({expected_step}) not found in plan: {plan}"
            );
            if expected_step.contains("Index Only Scan") {
                // Index Only Scan should only use an Index Cond for the scan, without any filtering.
                assert!(
                    !step.contains("Filter"),
                    "Extra filter found in index only scan which can be inefficient: {step}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_objects_filter_query_plan_efficiency() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let filters = ObjectFilter {
            type_filter: Some(TypeFilter::FullType(StructTag {
                address: SuiAddress::ZERO.into(),
                module: ident_str!("coin").to_owned(),
                name: ident_str!("Coin").to_owned(),
                type_params: vec![TypeTag::U8, TypeTag::Address],
            })),
            owner_filter: Some(SuiAddress::ZERO),
        };
        let query_without_cursor = build_object_ids_query(filters.clone(), Some(1000), None, 50);
        assert_match_query_plan(
            &[
                "Limit",
                "Nested Loop Anti Join",
                "Index Only Scan using obj_info_owner_inst on obj_info",
                "Index Only Scan using obj_info_pkey on obj_info o",
            ],
            &get_query_plan(&query_without_cursor, &mut conn).await,
        );

        let query_with_cursor = build_object_ids_query(
            filters,
            Some(1000),
            Some(Cursor {
                checkpoint: 1000,
                object_id: vec![0; 32],
            }),
            50,
        );
        assert_match_query_plan(
            &[
                "Limit",
                "Append",
                // First subquery
                "Limit",
                "Nested Loop Anti Join",
                "Index Only Scan using obj_info_owner_inst on obj_info",
                "Index Only Scan using obj_info_pkey on obj_info o",
                // Second subquery
                "Limit",
                "Nested Loop Anti Join",
                "Index Only Scan using obj_info_owner_inst on obj_info",
                "Index Only Scan using obj_info_pkey on obj_info o",
            ],
            &get_query_plan(&query_with_cursor, &mut conn).await,
        );
    }

    #[tokio::test]
    async fn test_latest_object_versions_query_plan_efficiency() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let object_ids = [vec![0; 32], vec![1; 32], vec![2; 32]];
        let view_checkpoint_number = Some(1000);
        let query = build_latest_object_versions_query(object_ids.iter(), view_checkpoint_number);
        let query_plan = get_query_plan(&query, &mut conn).await;
        assert_match_query_plan(
            &[
                "Append",
                "Subquery Scan on subquery0",
                "Limit",
                "Index Only Scan using obj_versions_id_cp_version on obj_versions",
                "Subquery Scan on subquery1",
                "Limit",
                "Index Only Scan using obj_versions_id_cp_version on obj_versions",
                "Subquery Scan on subquery2",
                "Limit",
                "Index Only Scan using obj_versions_id_cp_version on obj_versions",
            ],
            &query_plan,
        );
    }
}
