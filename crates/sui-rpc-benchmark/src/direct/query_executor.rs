// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use anyhow::Result;
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use rand::seq::SliceRandom;
use tokio_postgres::{types::ToSql, types::Type, NoTls, Row};

use crate::direct::benchmark_config::BenchmarkConfig;
use crate::direct::metrics::{BenchmarkResult, MetricsCollector};
use crate::direct::query_generator::BenchmarkQuery;

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
    pub query: BenchmarkQuery,
    pub rows: Vec<Vec<SqlValue>>,
    pub types: Vec<Type>,
}

pub struct QueryExecutor {
    pool: Pool<PostgresConnectionManager<NoTls>>,
    queries: Vec<BenchmarkQuery>,
    enriched_queries: Vec<EnrichedBenchmarkQuery>,
    config: BenchmarkConfig,
    metrics: MetricsCollector,
}

impl QueryExecutor {
    pub async fn new(
        db_url: &str,
        queries: Vec<BenchmarkQuery>,
        config: BenchmarkConfig,
    ) -> Result<Self> {
        let manager = PostgresConnectionManager::new_from_stringlike(db_url, NoTls)?;
        let pool = Pool::builder().build(manager).await?;

        Ok(Self {
            pool,
            queries,
            enriched_queries: Vec::new(),
            config,
            metrics: MetricsCollector::default(),
        })
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

    async fn enrich_query(&self, query: &BenchmarkQuery) -> Result<EnrichedBenchmarkQuery> {
        let client = self.pool.get().await?;
        let sql = format!(
            "SELECT DISTINCT {} FROM {} WHERE {} IS NOT NULL LIMIT 1000",
            query.needed_columns.join(", "),
            query.table_name,
            query.needed_columns[0]
        );

        let rows = client.query(&sql, &[]).await?;
        if rows.is_empty() {
            println!(
                "Warning: No sample data found for query on table {}, table is empty",
                query.table_name
            );
            return Ok(EnrichedBenchmarkQuery {
                query: query.clone(),
                rows: Vec::new(),
                types: query.needed_columns.iter().map(|_| Type::TEXT).collect(), // default type
            });
        }

        let types = rows[0].columns().iter().map(|c| c.type_().clone()).collect();
        let raw_rows = rows.iter().map(Self::row_to_values).collect();

        Ok(EnrichedBenchmarkQuery {
            query: query.clone(),
            rows: raw_rows,
            types,
        })
    }

    pub async fn initialize_samples(&mut self) -> Result<()> {
        for query in &self.queries.clone() {
            let enriched = self.enrich_query(query).await?;
            self.enriched_queries.push(enriched);
        }
        Ok(())
    }

    async fn worker_task(
        pool: Pool<PostgresConnectionManager<NoTls>>,
        enriched_queries: Vec<EnrichedBenchmarkQuery>,
        metrics: MetricsCollector,
        deadline: Instant,
    ) -> Result<()> {
        let client = pool.get().await?;
        while Instant::now() < deadline {
            let enriched = enriched_queries
                .choose(&mut rand::thread_rng())
                .ok_or_else(|| anyhow::anyhow!("No queries available"))?;

            let row = match enriched.rows.choose(&mut rand::thread_rng()) {
                Some(row) => row,
                None => {
                    // skip when the table is empty and thus no values to sample.
                    continue;
                }
            };

            let params: Vec<Box<dyn ToSql + Sync + Send>> = row
                .iter()
                .map(|val| match val {
                    SqlValue::Text(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Int4(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Int8(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Float8(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Bool(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Int2(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                    SqlValue::Bytea(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
                })
                .collect();
            let param_refs: Vec<&(dyn ToSql + Sync)> = params
                .iter()
                .map(|p| p.as_ref() as &(dyn ToSql + Sync))
                .collect();

            let query_str = enriched.query.query_template.clone();

            let start = Instant::now();
            let result = client.query(&query_str, &param_refs[..]).await;

            metrics.record_query(&enriched.query.table_name, start.elapsed(), result.is_err());
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<BenchmarkResult> {
        if self.enriched_queries.is_empty() {
            self.initialize_samples().await?;
        }

        println!(
            "Running benchmark with {} concurrent clients",
            self.config.concurrency
        );

        let start = Instant::now();
        let deadline = start + self.config.duration;

        let queries_per_worker = self.enriched_queries.chunks(
            (self.enriched_queries.len() + self.config.concurrency - 1) / self.config.concurrency,
        );

        let mut handles = Vec::new();
        for worker_queries in queries_per_worker {
            let pool = self.pool.clone();
            let worker_queries = worker_queries.to_vec();
            let metrics = self.metrics.clone();

            let handle = tokio::spawn(async move {
                QueryExecutor::worker_task(pool, worker_queries, metrics, deadline).await
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await??;
        }

        Ok(self.metrics.generate_report())
    }
}
