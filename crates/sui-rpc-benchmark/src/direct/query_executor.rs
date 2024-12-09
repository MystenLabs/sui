// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio_postgres::{types::ToSql, types::Type, Client, Row};

use crate::direct::query_generator::BenchmarkQuery;

pub struct QueryExecutor {
    db_client: Client,
    enriched_benchmark_queries: Vec<EnrichedBenchmarkQuery>,
}

#[derive(Debug)]
pub struct EnrichedBenchmarkQuery {
    pub query: BenchmarkQuery,
    pub rows: Vec<Row>,
}

impl QueryExecutor {
    pub async fn new(
        db_url: &str,
        benchmark_queries: Vec<BenchmarkQuery>,
    ) -> Result<Self, anyhow::Error> {
        let (client, connection) = tokio_postgres::connect(db_url, tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        let mut executor = Self {
            db_client: client,
            enriched_benchmark_queries: Vec::new(),
        };
        let mut enriched_queries = Vec::new();
        for query in benchmark_queries {
            let enriched = executor.enrich_query(&query).await?;
            enriched_queries.push(enriched);
        }
        executor.enriched_benchmark_queries = enriched_queries;

        Ok(executor)
    }

    pub async fn run(&self) -> Result<(), anyhow::Error> {
        println!(
            "Starting parallel execution of {} queries",
            self.enriched_benchmark_queries.len()
        );
        let futures: Vec<_> = self
            .enriched_benchmark_queries
            .iter()
            .map(|enriched| {
                println!("Executing query: {}", enriched.query.query_template);
                self.execute_query(enriched)
            })
            .collect();
        let results = futures::future::join_all(futures).await;

        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(rows) => println!(
                    "Query \n'{}'\n completed successfully with {} rows",
                    self.enriched_benchmark_queries[i].query.query_template,
                    rows.len()
                ),
                Err(e) => println!(
                    "Query \n'{}'\n failed with error: {}",
                    self.enriched_benchmark_queries[i].query.query_template, e
                ),
            }
        }
        println!("All benchmark queries completed");
        Ok(())
    }

    async fn enrich_query(
        &self,
        bq: &BenchmarkQuery,
    ) -> Result<EnrichedBenchmarkQuery, anyhow::Error> {
        // TODO(gegaowp): only fetch one row for quick execution, will configure and fetch more.
        let query = format!(
            "SELECT {} FROM {} LIMIT 1",
            bq.needed_columns.join(","),
            bq.table_name
        );
        println!("Enriched query: {}", query);
        let rows = self.db_client.query(&query, &[]).await?;

        Ok(EnrichedBenchmarkQuery {
            query: bq.clone(),
            rows,
        })
    }

    async fn execute_query(
        &self,
        query: &EnrichedBenchmarkQuery,
    ) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
        let mut all_results = Vec::new();
        for row in &query.rows {
            let params_vec = row_to_params(row);
            let value_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params_vec.iter().map(|v| v.as_ref()).collect();

            let results = self
                .db_client
                .query(&query.query.query_template, &value_refs)
                .await?;
            all_results.extend(results);
        }
        Ok(all_results)
    }
}

fn row_to_params(row: &Row) -> Vec<Box<dyn ToSql + Sync>> {
    let mut params: Vec<Box<dyn ToSql + Sync>> = Vec::new();

    for i in 0..row.len() {
        match row.columns()[i].type_() {
            &Type::TEXT | &Type::VARCHAR => {
                params.push(Box::new(row.get::<_, Option<String>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::INT4 => {
                params.push(Box::new(row.get::<_, Option<i32>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::INT8 => {
                params.push(Box::new(row.get::<_, Option<i64>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::FLOAT8 => {
                params.push(Box::new(row.get::<_, Option<f64>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::BOOL => {
                params.push(Box::new(row.get::<_, Option<bool>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::INT2 => {
                params.push(Box::new(row.get::<_, Option<i16>>(i)) as Box<dyn ToSql + Sync>)
            }
            &Type::BYTEA => {
                params.push(Box::new(row.get::<_, Option<Vec<u8>>>(i)) as Box<dyn ToSql + Sync>)
            }
            _ => panic!("Unsupported type: {:?}", row.columns()[i].type_()),
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_pg_temp_db::TempDb;
    use tokio_postgres::NoTls;

    #[tokio::test]
    async fn test_execute_enriched_query() -> Result<(), Box<dyn std::error::Error>> {
        let db = TempDb::new().unwrap();
        let url = db.database().url();
        let (client, connection) = tokio_postgres::connect(url.as_str(), NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        // Create test table and insert test data
        client
            .execute(
                "CREATE TABLE IF NOT EXISTS test_table (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await?;
        client
            .execute(
                "INSERT INTO test_table (id, name) VALUES ($1, $2)",
                &[
                    &1i32 as &(dyn tokio_postgres::types::ToSql + Sync),
                    &"test" as &(dyn tokio_postgres::types::ToSql + Sync),
                ],
            )
            .await?;
        // Create benchmark query
        let benchmark_query = BenchmarkQuery {
            query_template: "SELECT * FROM test_table WHERE id = $1 AND name = $2".to_string(),
            table_name: "test_table".to_string(),
            needed_columns: vec!["id".to_string(), "name".to_string()],
        };

        // Create executor and enrich query
        let executor = QueryExecutor::new(url.as_str(), vec![benchmark_query]).await?;
        let enriched_query = &executor.enriched_benchmark_queries[0];

        // Assert enriched query details match what we expect
        assert_eq!(
            enriched_query.query.query_template,
            "SELECT * FROM test_table WHERE id = $1 AND name = $2"
        );
        assert_eq!(enriched_query.query.table_name, "test_table");
        assert_eq!(
            enriched_query.query.needed_columns,
            vec!["id".to_string(), "name".to_string()]
        );
        assert_eq!(enriched_query.rows.len(), 1);

        // Execute enriched query
        let result = executor.execute_query(enriched_query).await?;

        // Verify result matches expected values
        assert_eq!(result.len(), 1);
        assert!(check_rows_consistency(&result[0], &enriched_query.rows[0])?);

        Ok(())
    }

    fn check_rows_consistency(row1: &Row, row2: &Row) -> Result<bool, Box<dyn std::error::Error>> {
        // Get column names for both rows
        let cols1: Vec<&str> = row1.columns().iter().map(|c| c.name()).collect();
        let cols2: Vec<&str> = row2.columns().iter().map(|c| c.name()).collect();

        // Find overlapping columns
        let common_cols: Vec<&str> = cols1
            .iter()
            .filter(|&col| cols2.contains(col))
            .cloned()
            .collect();

        // Check each common column for value equality
        for col in common_cols {
            // assert the column types match
            let col_type1 = row1
                .columns()
                .iter()
                .find(|c| c.name() == col)
                .map(|c| c.type_())
                .unwrap();
            let col_type2 = row2
                .columns()
                .iter()
                .find(|c| c.name() == col)
                .map(|c| c.type_())
                .unwrap();
            assert_eq!(
                col_type1, col_type2,
                "Column types should match for column {}",
                col
            );
            let col_type = col_type1;

            // assert the column values match
            if !compare_row_values(row1, row2, col, col_type)? {
                println!("Column '{}' has inconsistent values between rows", col);
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn compare_row_values(
        row1: &Row,
        row2: &Row,
        col: &str,
        col_type: &Type,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(match col_type {
            &Type::TEXT | &Type::VARCHAR => {
                row1.get::<_, String>(col) == row2.get::<_, String>(col)
            }
            &Type::INT4 => row1.get::<_, i32>(col) == row2.get::<_, i32>(col),
            &Type::INT8 => row1.get::<_, i64>(col) == row2.get::<_, i64>(col),
            &Type::FLOAT8 => row1.get::<_, f64>(col) == row2.get::<_, f64>(col),
            &Type::BOOL => row1.get::<_, bool>(col) == row2.get::<_, bool>(col),
            &Type::INT2 => row1.get::<_, i16>(col) == row2.get::<_, i16>(col),
            &Type::BYTEA => row1.get::<_, Vec<u8>>(col) == row2.get::<_, Vec<u8>>(col),
            _ => panic!("Unsupported type: {:?}", col_type),
        })
    }
}
