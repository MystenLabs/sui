// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module generates SQL query templates for benchmarking, including
/// query templates based on primary key columns and indexed columns.
///
/// The primary key query templates ("pk queries") select a row by each PK,
/// while the "index queries" filter by indexed columns. Instead
/// of returning just a list of tables and indexes, this module
/// returns a vector of QueryTemplate objects, each of which is
/// ready to be executed. This approach streamlines the pipeline
/// so we can directly run these queries as part of the benchmark.
use tokio_postgres::NoTls;
use tracing::{debug, info};
use url::Url;

#[derive(Debug, Clone)]
pub struct QueryTemplate {
    pub query_template: String,
    pub table_name: String,
    pub needed_columns: Vec<String>,
}

pub struct QueryTemplateGenerator {
    db_url: Url,
}

impl QueryTemplateGenerator {
    pub fn new(db_url: Url) -> Self {
        Self { db_url }
    }

    pub async fn generate_query_templates(&self) -> Result<Vec<QueryTemplate>, anyhow::Error> {
        let (client, connection) = tokio_postgres::connect(self.db_url.as_str(), NoTls).await?;
        tokio::spawn(connection);

        let pk_query = r#"
            SELECT 
                tc.table_name,  
                array_agg(kcu.column_name ORDER BY kcu.ordinal_position)::text[] as primary_key_columns  
            FROM information_schema.table_constraints tc  
            JOIN information_schema.key_column_usage kcu 
                ON tc.constraint_name = kcu.constraint_name  
            WHERE tc.constraint_type = 'PRIMARY KEY'
                AND tc.table_schema = 'public'
                AND tc.table_name != '__diesel_schema_migrations'
            GROUP BY tc.table_name
            ORDER BY tc.table_name;
        "#;

        let mut queries = Vec::new();
        let rows = client.query(pk_query, &[]).await?;
        let tables: Vec<String> = rows
            .iter()
            .map(|row| row.get::<_, String>("table_name"))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        info!(
            "Found {} active tables in database: {:?}",
            tables.len(),
            tables
        );

        // Process primary key queries - now each row has all columns for a table
        for row in rows {
            let table: String = row.get("table_name");
            let pk_columns: Vec<String> = row.get("primary_key_columns");
            queries.push(self.create_pk_benchmark_query(&table, &pk_columns));
        }

        let idx_query = r#"
            SELECT 
                t.relname AS table_name,
                i.relname AS index_name,
                array_agg(a.attname ORDER BY k.i) AS column_names
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON ix.indexrelid = i.oid
            JOIN pg_attribute a ON t.oid = a.attrelid
            JOIN generate_subscripts(ix.indkey, 1) k(i) ON a.attnum = ix.indkey[k.i]
            WHERE t.relkind = 'r'
                AND t.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'public')
                AND NOT ix.indisprimary
                AND t.relname != '__diesel_schema_migrations'
            GROUP BY t.relname, i.relname
            ORDER BY t.relname, i.relname;
        "#;

        let rows = client.query(idx_query, &[]).await?;
        for row in rows {
            let table: String = row.get("table_name");
            let columns: Vec<String> = row.get("column_names");
            queries.push(self.create_index_benchmark_query(&table, &columns));
        }

        debug!("Generated {} queries:", queries.len());
        for (i, query) in queries.iter().enumerate() {
            debug!(
                "  {}. Table: {}, Template: {}",
                i + 1,
                query.table_name,
                query.query_template
            );
        }

        Ok(queries)
    }

    /// An example query template:
    /// SELECT * FROM tx_kinds WHERE tx_kind = $1 AND tx_sequence_number = $2 LIMIT 1
    fn create_pk_benchmark_query(&self, table: &str, columns: &[String]) -> QueryTemplate {
        let conditions = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        QueryTemplate {
            query_template: format!("SELECT * FROM {} WHERE {} LIMIT 1", table, conditions),
            table_name: table.to_string(),
            needed_columns: columns.to_vec(),
        }
    }

    fn create_index_benchmark_query(&self, table: &str, columns: &[String]) -> QueryTemplate {
        let conditions = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        QueryTemplate {
            query_template: format!("SELECT * FROM {} WHERE {} LIMIT 50", table, conditions),
            table_name: table.to_string(),
            needed_columns: columns.to_vec(),
        }
    }
}
