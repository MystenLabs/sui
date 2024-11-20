// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::{Statement, CreateTable, TableConstraint, CreateIndex, ColumnOption};

pub struct QueryGenerator;

impl QueryGenerator {
    pub fn read_migration_sql() -> Result<String, Box<dyn std::error::Error>> {
        let mut combined_sql = String::new();

        // Get all up.sql files from migrations directory
        let migration_files = std::fs::read_dir("crates/sui-indexer-alt/migrations")?;

        for file in migration_files {
            let file = file?;
            let path = file.path();
            
            // Only process up.sql files
            if path.is_file() && 
               path.file_name()
                   .and_then(|f| f.to_str())
                   .map(|s| s.ends_with("up.sql"))
                   .unwrap_or(false) {
                
                // Read and append SQL content
                let sql = std::fs::read_to_string(&path)?;
                combined_sql.push_str(&sql);
                combined_sql.push('\n');
            }
        }

        Ok(combined_sql)
    }

    pub fn read_sqls(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // Read all SQL files from migrations directory
        let migration_dir = std::fs::read_dir("crates/sui-indexer-alt/migrations")?;
        let mut sqls = Vec::new();

        for entry in migration_dir {
            let entry = entry?;
            let path = entry.path();

            // Only process up.sql files
            if path.is_file() && 
               path.file_name()
                   .and_then(|f| f.to_str())
                   .map(|s| s.ends_with("up.sql"))
                   .unwrap_or(false) {
                let sql = std::fs::read_to_string(&path)?;
                sqls.push(sql);
            }
        }

        Ok(sqls)
    }

    pub fn generate_benchmark_queries(&self) -> Result<Vec<BenchmarkQuery>, Box<dyn std::error::Error>> {
        // First read all SQL files
        let sqls = self.read_sqls()?;
        
        // Convert each SQL file to benchmark queries and flatten results
        let mut all_queries = Vec::new();
        for sql in sqls {
            let queries = sql_to_benchmark_queries(&sql)?;
            all_queries.extend(queries);
        }

        Ok(all_queries)
    }

}

#[derive(Debug, Clone)]
pub struct BenchmarkQuery {
    pub query_template: String,
    pub table_name: String,
    pub needed_columns: Vec<String>,
}


fn sql_to_benchmark_queries(sql: &str) -> Result<Vec<BenchmarkQuery>, Box<dyn std::error::Error>> {
    let dialect = PostgreSqlDialect {};
    let statements = Parser::parse_sql(&dialect, sql)?;
    
    let mut table_name = String::new();
    let mut primary_keys = Vec::new();
    let mut indexes = Vec::new();

    for stmt in statements {
        match stmt {
            Statement::CreateTable(CreateTable { name, columns, constraints, .. }) => {
                table_name = name.to_string();
                // Extract primary keys from table PRIMARY KEY constraint, for example:
                // CREATE TABLE test_table (
                //     id BIGINT,
                //     name VARCHAR(255),
                //     value INTEGER,
                //     PRIMARY KEY (id)
                // );
                for constraint in &constraints {
                    if let TableConstraint::PrimaryKey { columns, .. } = constraint {
                        primary_keys = columns.iter()
                            .map(|c| c.to_string())
                            .collect();
                    }
                }
                // If table PRIMARY KEY constraint is not defined, check column constraints for PRIMARY KEY option, for example:
                // CREATE TABLE test_table (
                //     id BIGINT PRIMARY KEY,
                //     name VARCHAR(255),
                //     value INTEGER
                // );
                if primary_keys.is_empty() {
                    for column in &columns {
                        for option in &column.options {
                            if let ColumnOption::Unique { is_primary, .. } = &option.option {
                                if *is_primary {
                                    primary_keys.push(column.name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Statement::CreateIndex(CreateIndex { name: _, table_name: _idx_table, columns, .. }) => {
                indexes.push(columns.iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>());
            }
            _ => {}
        }
    }

    // Generate queries
    let mut queries = Vec::new();

    // 1. Primary key lookup query
    if !primary_keys.is_empty() {
        let pk_conditions = primary_keys.iter()
            .map(|pk| format!("{} = ${}", pk, pk))
            .collect::<Vec<_>>()
            .join(" AND ");
        
        queries.push(BenchmarkQuery {
            query_template: format!("SELECT * FROM {} WHERE {}", table_name, pk_conditions),
            table_name: table_name.clone(),
            needed_columns: primary_keys.clone(),
        });
    }

    // 2. Index-based queries
    for idx_columns in indexes {
        let conditions = idx_columns.iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        queries.push(BenchmarkQuery {
            query_template: format!("SELECT * FROM {} WHERE {}", 
                table_name, conditions),
            table_name: table_name.clone(),
            needed_columns: idx_columns,
        });
    }

    Ok(queries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_queries() -> Result<(), Box<dyn std::error::Error>> {
        let sql = r#"
            CREATE TABLE test_table (
                id BIGINT PRIMARY KEY,
                name VARCHAR(255),
                value INTEGER
            );
            
            CREATE INDEX idx_name ON test_table(name);
            CREATE INDEX idx_value ON test_table(value);
        "#;

        let queries = sql_to_benchmark_queries(sql)?;

        // Should generate 3 queries - one for PK lookup and two for indexes
        assert_eq!(queries.len(), 3);

        // Verify PK query
        assert_eq!(queries[0].query_template, "SELECT * FROM test_table WHERE id = $id");
        assert_eq!(queries[0].table_name, "test_table");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify index queries
        assert_eq!(queries[1].query_template, "SELECT * FROM test_table WHERE name = $1");
        assert_eq!(queries[1].table_name, "test_table");
        assert_eq!(queries[1].needed_columns, vec!["name"]);

        assert_eq!(queries[2].query_template, "SELECT * FROM test_table WHERE value = $1");
        assert_eq!(queries[2].table_name, "test_table");
        assert_eq!(queries[2].needed_columns, vec!["value"]);

        Ok(())
    }

    #[test]
    fn test_generate_queries_with_table_pk() -> Result<(), Box<dyn std::error::Error>> {
        let sql = r#"
            CREATE TABLE test_table2 (
                id BIGINT,
                name VARCHAR(255),
                value INTEGER,
                PRIMARY KEY (id)
            );
            
            CREATE INDEX idx_name ON test_table2(name);
            CREATE INDEX idx_value ON test_table2(value);
        "#;

        let queries = sql_to_benchmark_queries(sql)?;

        // Should generate 3 queries - one for PK lookup and two for indexes
        assert_eq!(queries.len(), 3);

        // Verify PK query
        assert_eq!(queries[0].query_template, "SELECT * FROM test_table2 WHERE id = $id");
        assert_eq!(queries[0].table_name, "test_table2");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify index queries
        assert_eq!(queries[1].query_template, "SELECT * FROM test_table2 WHERE name = $1");
        assert_eq!(queries[1].table_name, "test_table2");
        assert_eq!(queries[1].needed_columns, vec!["name"]);

        assert_eq!(queries[2].query_template, "SELECT * FROM test_table2 WHERE value = $1");
        assert_eq!(queries[2].table_name, "test_table2");
        assert_eq!(queries[2].needed_columns, vec!["value"]);

        Ok(())
    }

    #[test]
    fn test_generate_queries_with_multi_column_indexes() -> Result<(), Box<dyn std::error::Error>> {
        let sql = r#"
            CREATE TABLE test_table3 (
                id BIGINT,
                name VARCHAR(255),
                value INTEGER,
                category VARCHAR(50),
                PRIMARY KEY (id)
            );
            
            CREATE INDEX idx_name_value ON test_table3(name, value);
            CREATE INDEX idx_category_value ON test_table3(category, value);
        "#;

        let queries = sql_to_benchmark_queries(sql)?;

        // Should generate 3 queries - one for PK and two for composite indexes
        assert_eq!(queries.len(), 3);

        // Verify PK query
        assert_eq!(queries[0].query_template, "SELECT * FROM test_table3 WHERE id = $id");
        assert_eq!(queries[0].table_name, "test_table3");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify multi-column index queries
        assert_eq!(queries[1].query_template, "SELECT * FROM test_table3 WHERE name = $1 AND value = $2");
        assert_eq!(queries[1].table_name, "test_table3");
        assert_eq!(queries[1].needed_columns, vec!["name", "value"]);

        assert_eq!(queries[2].query_template, "SELECT * FROM test_table3 WHERE category = $1 AND value = $2");
        assert_eq!(queries[2].table_name, "test_table3");
        assert_eq!(queries[2].needed_columns, vec!["category", "value"]);

        Ok(())
    }
}
