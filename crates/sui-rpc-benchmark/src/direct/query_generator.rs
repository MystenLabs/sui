// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio_postgres::NoTls;

pub struct QueryGenerator {
    pub db_url: String,
}

#[derive(Debug, Clone)]
pub struct BenchmarkQuery {
    pub query_template: String,
    pub table_name: String,
    pub needed_columns: Vec<String>,
}

impl QueryGenerator {
    async fn get_tables_and_indexes(&self) -> Result<Vec<BenchmarkQuery>, anyhow::Error> {
        let (client, connection) = tokio_postgres::connect(&self.db_url, NoTls).await?;
        tokio::spawn(connection);
        let tables_query = r#"
            SELECT tablename 
            FROM pg_tables 
            WHERE schemaname = 'public' 
            AND tablename != '__diesel_schema_migrations'
            ORDER BY tablename;
        "#;
        let tables: Vec<String> = client
            .query(tables_query, &[])
            .await?
            .iter()
            .map(|row| row.get::<_, String>(0))
            .collect();
        println!(
            "Found {} active tables in database: {:?}",
            tables.len(),
            tables
        );

        let pk_query = r#"
            SELECT tc.table_name, kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu 
                ON tc.constraint_name = kcu.constraint_name
            WHERE tc.constraint_type = 'PRIMARY KEY'
                AND tc.table_schema = 'public'
                AND tc.table_name != '__diesel_schema_migrations'
            ORDER BY tc.table_name, kcu.ordinal_position;
        "#;

        let mut queries = Vec::new();
        let rows = client.query(pk_query, &[]).await?;
        let mut current_table = String::new();
        let mut pk_columns = Vec::new();

        for row in rows {
            let table: String = row.get("table_name");
            let column: String = row.get("column_name");

            if table != current_table && !current_table.is_empty() {
                queries.push(self.create_pk_query(&current_table, &pk_columns));
                pk_columns.clear();
            }

            current_table = table;
            pk_columns.push(column);
        }
        if !pk_columns.is_empty() {
            queries.push(self.create_pk_query(&current_table, &pk_columns));
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
            queries.push(self.create_index_query(&table, &columns));
        }

        println!("\nGenerated {} queries:", queries.len());
        for (i, query) in queries.iter().enumerate() {
            println!(
                "  {}. Table: {}, Template: {}",
                i + 1,
                query.table_name,
                query.query_template
            );
        }

        Ok(queries)
    }

    fn create_pk_query(&self, table: &str, columns: &[String]) -> BenchmarkQuery {
        let conditions = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        BenchmarkQuery {
            query_template: format!("SELECT * FROM {} WHERE {} LIMIT 1", table, conditions),
            table_name: table.to_string(),
            needed_columns: columns.to_vec(),
        }
    }

    fn create_index_query(&self, table: &str, columns: &[String]) -> BenchmarkQuery {
        let conditions = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        BenchmarkQuery {
            query_template: format!("SELECT * FROM {} WHERE {} LIMIT 50", table, conditions),
            table_name: table.to_string(),
            needed_columns: columns.to_vec(),
        }
    }

    pub async fn generate_benchmark_queries(&self) -> Result<Vec<BenchmarkQuery>, anyhow::Error> {
        let queries = self.get_tables_and_indexes().await?;
        Ok(queries)
    }
}
