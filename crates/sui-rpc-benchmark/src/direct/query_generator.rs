// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sqlparser::ast::{ColumnOption, CreateIndex, CreateTable, Statement, TableConstraint};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::path::PathBuf;

pub struct QueryGenerator {
    pub migration_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BenchmarkQuery {
    pub query_template: String,
    pub table_name: String,
    pub needed_columns: Vec<String>,
}

impl QueryGenerator {
    fn read_sqls(&self) -> Result<Vec<String>, anyhow::Error> {
        let migration_path = self.migration_path.to_str().unwrap();
        let sqls = Self::read_sql_impl(std::path::Path::new(migration_path))?;
        println!("Read {} up.sql files from migrations directory", sqls.len());
        Ok(sqls)
    }

    fn read_sql_impl(dir: &std::path::Path) -> Result<Vec<String>, anyhow::Error> {
        let mut sqls: Vec<String> = Vec::new();
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let inner_sqls = Self::read_sql_impl(&path)?;
                    sqls.extend(inner_sqls);
                } else if path.is_file()
                    && path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .map(|s| s.ends_with("up.sql"))
                        .unwrap_or(false)
                {
                    let sql = std::fs::read_to_string(&path)?;
                    sqls.push(sql);
                }
            }
        }
        Ok(sqls)
    }

    pub fn generate_benchmark_queries(&self) -> Result<Vec<BenchmarkQuery>, anyhow::Error> {
        let sqls = self.read_sqls()?;
        let mut benchmark_queries = Vec::new();
        for sql in sqls {
            let queries = sql_to_benchmark_queries(&sql)?;
            benchmark_queries.extend(queries);
        }
        Ok(benchmark_queries)
    }
}

fn sql_to_benchmark_queries(sql: &str) -> Result<Vec<BenchmarkQuery>, anyhow::Error> {
    let dialect = PostgreSqlDialect {};
    let statements = Parser::parse_sql(&dialect, sql)?;

    let mut tables = Vec::new();
    let mut indexes = Vec::new();

    for stmt in statements {
        match stmt {
            Statement::CreateTable(CreateTable {
                name,
                columns,
                constraints,
                ..
            }) => {
                let table_name = name.to_string();
                let mut primary_keys = Vec::new();
                // Extract primary keys from table PRIMARY KEY constraint, for example:
                // CREATE TABLE test_table (
                //     id BIGINT,
                //     name VARCHAR(255),
                //     value INTEGER,
                //     PRIMARY KEY (id)
                // );
                for constraint in &constraints {
                    if let TableConstraint::PrimaryKey { columns, .. } = constraint {
                        primary_keys = columns.iter().map(|c| c.to_string()).collect();
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

                tables.push((table_name, primary_keys));
            }
            Statement::CreateIndex(CreateIndex {
                name: _,
                table_name: idx_table,
                columns,
                ..
            }) => {
                indexes.push((
                    idx_table.to_string(),
                    columns.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
                ));
            }
            _ => {}
        }
    }

    let mut queries = Vec::new();
    // 1. Primary key lookup queries
    for (table_name, primary_keys) in tables {
        if !primary_keys.is_empty() {
            let pk_conditions = primary_keys
                .iter()
                .enumerate()
                .map(|(i, pk)| format!("{} = ${}", pk, i + 1))
                .collect::<Vec<_>>()
                .join(" AND ");

            queries.push(BenchmarkQuery {
                query_template: format!(
                    "SELECT * FROM {} WHERE {} LIMIT 1",
                    table_name, pk_conditions
                ),
                table_name: table_name.clone(),
                needed_columns: primary_keys,
            });
        }
    }
    // 2. Index-based queries
    for (table_name, idx_columns) in indexes {
        let conditions = idx_columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        queries.push(BenchmarkQuery {
            query_template: format!("SELECT * FROM {} WHERE {} LIMIT 50", table_name, conditions),
            table_name,
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
        assert_eq!(
            queries[0].query_template,
            "SELECT * FROM test_table WHERE id = $1 LIMIT 1"
        );
        assert_eq!(queries[0].table_name, "test_table");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify index queries
        assert_eq!(
            queries[1].query_template,
            "SELECT * FROM test_table WHERE name = $1 LIMIT 50"
        );
        assert_eq!(queries[1].table_name, "test_table");
        assert_eq!(queries[1].needed_columns, vec!["name"]);

        assert_eq!(
            queries[2].query_template,
            "SELECT * FROM test_table WHERE value = $1 LIMIT 50"
        );
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
        assert_eq!(
            queries[0].query_template,
            "SELECT * FROM test_table2 WHERE id = $1 LIMIT 1"
        );
        assert_eq!(queries[0].table_name, "test_table2");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify index queries
        assert_eq!(
            queries[1].query_template,
            "SELECT * FROM test_table2 WHERE name = $1 LIMIT 50"
        );
        assert_eq!(queries[1].table_name, "test_table2");
        assert_eq!(queries[1].needed_columns, vec!["name"]);

        assert_eq!(
            queries[2].query_template,
            "SELECT * FROM test_table2 WHERE value = $1 LIMIT 50"
        );
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
        assert_eq!(
            queries[0].query_template,
            "SELECT * FROM test_table3 WHERE id = $1 LIMIT 1"
        );
        assert_eq!(queries[0].table_name, "test_table3");
        assert_eq!(queries[0].needed_columns, vec!["id"]);

        // Verify multi-column index queries
        assert_eq!(
            queries[1].query_template,
            "SELECT * FROM test_table3 WHERE name = $1 AND value = $2 LIMIT 50"
        );
        assert_eq!(queries[1].table_name, "test_table3");
        assert_eq!(queries[1].needed_columns, vec!["name", "value"]);

        assert_eq!(
            queries[2].query_template,
            "SELECT * FROM test_table3 WHERE category = $1 AND value = $2 LIMIT 50"
        );
        assert_eq!(queries[2].table_name, "test_table3");
        assert_eq!(queries[2].needed_columns, vec!["category", "value"]);

        Ok(())
    }
}
