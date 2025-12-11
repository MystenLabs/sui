// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Snowflake-based checkpoint progress reader.

use anyhow::{Result, anyhow};

use super::MaxCheckpointReader;

/// Reads the maximum checkpoint from a Snowflake table.
pub struct SnowflakeMaxCheckpointReader {
    query: String,
    api: snowflake_api::SnowflakeApi,
}

impl SnowflakeMaxCheckpointReader {
    /// Creates a new Snowflake checkpoint reader.
    pub async fn new(
        account_identifier: &str,
        warehouse: &str,
        database: &str,
        schema: &str,
        user: &str,
        role: &str,
        passwd: &str,
        table_id: &str,
        col_id: &str,
    ) -> Result<Self> {
        let api = snowflake_api::SnowflakeApi::with_password_auth(
            account_identifier,
            Some(warehouse),
            Some(database),
            Some(schema),
            user,
            Some(role),
            passwd,
        )
        .expect("Failed to build sf api client");
        Ok(SnowflakeMaxCheckpointReader {
            query: format!("SELECT max({}) from {}", col_id, table_id),
            api,
        })
    }
}

#[async_trait::async_trait]
impl MaxCheckpointReader for SnowflakeMaxCheckpointReader {
    async fn max_checkpoint(&self) -> Result<i64> {
        use arrow::array::Int32Array;
        use snowflake_api::QueryResult;

        let res = self.api.exec(&self.query).await?;
        match res {
            QueryResult::Arrow(a) => {
                if let Some(record_batch) = a.first() {
                    let col = record_batch.column(0);
                    let col_array = col
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .expect("Failed to downcast arrow column");
                    Ok(col_array.value(0) as i64)
                } else {
                    Ok(-1)
                }
            }
            QueryResult::Json(_j) => Err(anyhow!("Unexpected query result")),
            QueryResult::Empty => Err(anyhow!("Unexpected query result")),
        }
    }
}
