// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module enriches query templates with real data from the database.
/// This enrichment ensures that when we run the benchmark:
/// - We use realistic data values that actually exist in the database:
/// - We have a pool of valid values to randomly select from during execution.
use anyhow::Result;
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use tokio_postgres::{types::Type, NoTls, Row};
use tracing::warn;
use url::Url;

use crate::direct::query_template_generator::QueryTemplate;

#[derive(Clone, Debug)]
pub enum SqlValue {
    Text(Option<String>),
    Int4(Option<i32>),
    Int8(Option<i64>),
    Float8(Option<f64>),
    Bool(Option<bool>),
    Int2(Option<i16>),
    Bytea(Option<Vec<u8>>),
}

#[derive(Debug, Clone)]
pub struct EnrichedBenchmarkQuery {
    pub query: QueryTemplate,
    pub rows: Vec<Vec<SqlValue>>,
    pub types: Vec<Type>,
}

pub struct QueryEnricher {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

impl QueryEnricher {
    pub async fn new(db_url: &Url) -> Result<Self> {
        let manager = PostgresConnectionManager::new_from_stringlike(db_url.as_str(), NoTls)?;
        let pool = Pool::builder().build(manager).await?;
        Ok(Self { pool })
    }

    fn row_to_values(row: &Row) -> Vec<SqlValue> {
        (0..row.len())
            .map(|i| match row.columns()[i].type_() {
                &Type::TEXT | &Type::VARCHAR => SqlValue::Text(row.get(i)),
                &Type::INT4 => SqlValue::Int4(row.get(i)),
                &Type::INT8 => SqlValue::Int8(row.get(i)),
                &Type::FLOAT8 => SqlValue::Float8(row.get(i)),
                &Type::BOOL => SqlValue::Bool(row.get(i)),
                &Type::INT2 => SqlValue::Int2(row.get(i)),
                &Type::BYTEA => SqlValue::Bytea(row.get(i)),
                ty => panic!("Unsupported type: {:?}", ty),
            })
            .collect()
    }

    async fn enrich_query(&self, query: &QueryTemplate) -> Result<EnrichedBenchmarkQuery> {
        let client = self.pool.get().await?;
        let sql = format!(
            "SELECT {} FROM {} WHERE {} IS NOT NULL LIMIT 1000",
            query.needed_columns.join(", "),
            query.table_name,
            query.needed_columns[0]
        );

        let rows = client.query(&sql, &[]).await?;
        let Some(first_row) = rows.first() else {
            warn!(
                table = query.table_name,
                "No sample data found for query on table, table is empty."
            );
            return Ok(EnrichedBenchmarkQuery {
                query: query.clone(),
                rows: Vec::new(),
                types: query.needed_columns.iter().map(|_| Type::TEXT).collect(), // default type
            });
        };
        let types = first_row
            .columns()
            .iter()
            .map(|c| c.type_().clone())
            .collect();
        let raw_rows = rows.iter().map(Self::row_to_values).collect();

        Ok(EnrichedBenchmarkQuery {
            query: query.clone(),
            rows: raw_rows,
            types,
        })
    }

    pub async fn enrich_queries(
        &self,
        queries: Vec<QueryTemplate>,
    ) -> Result<Vec<EnrichedBenchmarkQuery>> {
        let mut enriched_queries = Vec::new();
        for query in queries {
            let enriched = self.enrich_query(&query).await?;
            enriched_queries.push(enriched);
        }
        Ok(enriched_queries)
    }
}
