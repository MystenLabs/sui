// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::direct::query_generator::BenchmarkQuery;
use tokio_postgres::{types::Type, Client, Row};


#[derive(Debug)]
pub struct EnrichedBenchmarkQuery {
    pub query: BenchmarkQuery,
    pub values: Vec<(Type, String)>,
}


pub struct QueryExecutor {
    db_client: Client,
    enriched_benchmark_queries: Vec<EnrichedBenchmarkQuery>,
}

impl QueryExecutor {
    pub async fn new(db_url: &str, benchmark_queries: Vec<BenchmarkQuery>) -> Result<Self, Box<dyn std::error::Error>> {
        // Connect to the database
        let (client, connection) = tokio_postgres::connect(db_url, tokio_postgres::NoTls).await?;

        // Spawn connection management task
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        // Create executor instance
        let mut executor = Self {
            db_client: client,
            enriched_benchmark_queries: Vec::new(),
        };

        // Enrich all benchmark queries
        let mut enriched_queries = Vec::new();
        for query in benchmark_queries {
            let enriched = executor.enrich_query(&query).await?;
            enriched_queries.push(enriched);
        }
        executor.enriched_benchmark_queries = enriched_queries;

        Ok(executor)
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn row_to_columns(row: &Row) -> Vec<(Type, String)> {
        row.columns()
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let col_type = col.type_().clone();
                let value = match *col.type_() {
                    Type::TEXT | Type::VARCHAR => row.get::<usize, String>(i),
                    Type::INT4 => row.get::<usize, i32>(i).to_string(),
                    Type::FLOAT8 => row.get::<usize, f64>(i).to_string(),
                    Type::BOOL => row.get::<usize, bool>(i).to_string(),
                    Type::INT8 => row.get::<usize, i64>(i).to_string(),
                    Type::INT2 => row.get::<usize, i16>(i).to_string(),
                    Type::BYTEA => format!("{:?}", row.get::<usize, Vec<u8>>(i)),
                    _ => "<unsupported type>".to_string(), // Fallback for unsupported types
                };
                (col_type, value)
            })
            .collect()
    }

    pub async fn enrich_query(&self, bq: &BenchmarkQuery) -> Result<EnrichedBenchmarkQuery, Box<dyn std::error::Error>> {
        let query = format!("SELECT {} FROM {} LIMIT 1", bq.needed_columns.join(","), bq.table_name);
        let rows = self.db_client.query(&query, &[]).await?;

        // here columns are vector of column name and column value
        let columns = Self::row_to_columns(&rows[0]);
        println!("Columns: {:?}", columns);

        Ok(EnrichedBenchmarkQuery { query: bq.clone(), values: columns })
    }

    pub async fn execute_query(&self, query: &EnrichedBenchmarkQuery) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
        // Convert string values to proper types based on the parameter type
        let mut typed_values: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = Vec::new();
        for (col_type, value_str) in query.values.iter() {
            let typed_value: Box<dyn tokio_postgres::types::ToSql + Sync> = match col_type.clone() {
                Type::INT4 => Box::new(value_str.parse::<i32>().unwrap()),
                Type::INT8 => Box::new(value_str.parse::<i64>().unwrap()),
                Type::TEXT | Type::VARCHAR => Box::new(value_str.to_string()),
                Type::FLOAT8 => Box::new(value_str.parse::<f64>().unwrap()),
                Type::BOOL => Box::new(value_str.parse::<bool>().unwrap()),
                Type::INT2 => Box::new(value_str.parse::<i16>().unwrap()),
                Type::BYTEA => Box::new(hex::decode(value_str).unwrap()),
                _ => {
                    panic!("Unsupported type: {:?}", col_type);
                },
            };
            typed_values.push(typed_value);
        }
    
        // Create a vector of references to the typed values
        let value_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_values.iter()
            .map(|v| v.as_ref())
            .collect();
    
        let query_template = query.query.query_template.clone();
        Self::fetch_values(&self.db_client, &query_template, &value_refs).await
    }

    async fn fetch_values(client: &tokio_postgres::Client, query: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Vec<Row>, tokio_postgres::Error> {
        println!("Executing query: {}", query);
        println!("With parameters: {:?}", params);
        let rows = client.query(query, params).await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_postgres::NoTls;
    use sui_pg_temp_db::TempDb;

    #[tokio::test]
    async fn test_execute_enriched_query() -> Result<(), Box<dyn std::error::Error>> {
        // Set up test database connection
        let db = TempDb::new().unwrap();
        let url = db.database().url();
        let (client, connection) = tokio_postgres::connect(url.as_str(), NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        // Create test table and insert test data
        client.execute(
            "CREATE TABLE IF NOT EXISTS test_table (id INTEGER PRIMARY KEY, name TEXT)",
            &[],
        ).await?;
        client.execute(
            "INSERT INTO test_table (id, name) VALUES ($1, $2)",
            &[&1i32 as &(dyn tokio_postgres::types::ToSql + Sync), &"test" as &(dyn tokio_postgres::types::ToSql + Sync)],
        ).await?;

        // Create benchmark query
        let benchmark_query = BenchmarkQuery {
            query_template: "SELECT * FROM test_table WHERE id = $1 AND name = $2".to_string(),
            table_name: "test_table".to_string(),
            needed_columns: vec!["id".to_string(), "name".to_string()],
        };

        // Create executor and enrich query
        let executor = QueryExecutor {
            db_client: client,
            enriched_benchmark_queries: Vec::new(),
        };
        let enriched_query = executor.enrich_query(&benchmark_query).await?;
        // Verify enriched query has correct values
        assert_eq!(enriched_query.query.query_template, "SELECT * FROM test_table WHERE id = $1 AND name = $2");
        // assert_eq!(enriched_query.query.table_name, "test_table");
        // assert_eq!(enriched_query.query.needed_columns, vec!["id", "name"]);
        // assert_eq!(enriched_query.values.len(), 1);

        // Print out enriched query details
        println!("Enriched Query Template: {}", enriched_query.query.query_template);
        println!("Table Name: {}", enriched_query.query.table_name);
        println!("Needed Columns: {:?}", enriched_query.query.needed_columns);
        println!("Number of Values: {:?}", enriched_query.values);

        // Execute enriched query
        let result = executor.execute_query(&enriched_query).await?;
        
        // Verify we got results back
        assert!(!result.is_empty());
        
        // Verify the returned data matches what we inserted
        let row = &result[0];
        assert_eq!(row.get::<_, i32>("id"), 1);
        assert_eq!(row.get::<_, String>("name"), "test");

        Ok(())
    }
}

