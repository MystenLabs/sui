// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    pg::Pg,
    query_builder::{bind_collector::RawBytesBindCollector, QueryBuilder, QueryFragment},
    PgConnection, RunQueryDsl,
};
use regex::Regex;
use sui_indexer::schema_v2::query_cost;

/// Extracts the raw sql query string from a diesel query
/// and replaces all the parameters with '0'
pub fn raw_sql_string_values_set(
    query: &dyn QueryFragment<Pg>,
) -> Result<String, crate::error::Error> {
    // This lets us get the underlying sql query param types & values
    // Currently unused because we set all vals to '0' but want to keep for future reference

    // let mut bind_collector = RawBytesBindCollector::<Pg>::new();
    // QueryFragment::<Pg>::collect_binds(
    //     &query,
    //     &mut bind_collector,
    //     pg_connection as &mut dyn diesel::pg::PgMetadataLookup,
    //     &Pg,
    // )?;

    // // These hold the real values of the parameters
    // // but we cannot repr them as needed since we dont have proper
    // // type info
    // // So use '0' for all
    // // Todo: could using shorter strings imply lower cost e.g for searching? We dont want that
    // let _binds = &bind_collector.binds;
    // let metadata = &bind_collector.metadata;
    // let num_params = metadata.len();

    let mut query_builder = <diesel::pg::Pg as diesel::backend::Backend>::QueryBuilder::default();
    QueryFragment::<Pg>::to_sql(&query, &mut query_builder, &Pg).map_err(|e| {
        crate::error::Error::Internal(format!(
            "Failed to extract raw sql string from query: {}",
            e
        ))
    })?;
    let sql: String = query_builder.finish();

    let re = Regex::new(r"\$(\d+)")
        .map_err(|e| crate::error::Error::Internal(format!("Failed create valid regex: {}", e)))?;
    Ok(re.replace_all(&sql, "'0'").to_string())
}

pub fn extract_cost(
    query: &dyn QueryFragment<Pg>,
    pg_connection: &mut PgConnection,
) -> Result<f64, crate::error::Error> {
    let raw_sql_string = raw_sql_string_values_set(query)?;
    diesel::select(query_cost(&raw_sql_string))
        .get_result::<f64>(pg_connection)
        .map_err(|e| {
            crate::error::Error::Internal(format!(
                "Unable to run query_cost function to determine query cost for {}: {}",
                raw_sql_string, e
            ))
        })
}

/// Creates a prepared statement and then runs explain on it
/// to get the query plan
/// Currently unused in favor of calling PG function directly
pub fn _create_explain_query(
    query: &dyn QueryFragment<Pg>,
    pg_connection: &mut PgConnection,
) -> diesel::QueryResult<(String, String)> {
    let mut bind_collector = RawBytesBindCollector::<Pg>::new();

    let mut query_builder = <diesel::pg::Pg as diesel::backend::Backend>::QueryBuilder::default();
    QueryFragment::<Pg>::to_sql(&query, &mut query_builder, &Pg)?;
    let sql = query_builder.finish();

    QueryFragment::<Pg>::collect_binds(
        &query,
        &mut bind_collector,
        pg_connection as &mut dyn diesel::pg::PgMetadataLookup,
        &Pg,
    )?;

    // These hold the real values of the parameters
    // but we cannot repr them as needed since we dont have proper
    // type info
    // So use '0' for all
    // Todo: could using shorter strings imply lower cost e.g for searching? We dont want that
    let _binds = &bind_collector.binds;
    let metadata = &bind_collector.metadata;
    let num_params = metadata.len();

    let prep = format!(
        "PREPARE myfun ({}) AS \n    {};\n",
        vec!["unknown"; num_params].join(", "),
        sql
    );
    let exec = format!(
        "EXPLAIN (FORMAT JSON) EXECUTE myfun ({});",
        vec!["'0'"; num_params].join(", ")
    );
    Ok((prep, exec))
}
