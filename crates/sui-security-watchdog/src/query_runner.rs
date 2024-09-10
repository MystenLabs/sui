// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SecurityWatchdogConfig;
use anyhow::anyhow;
use arrow_array::cast::AsArray;
use arrow_array::types::{
    Decimal128Type, Float16Type, Float32Type, Float64Type, Int16Type, Int32Type, Int64Type,
    Int8Type, UInt16Type, UInt32Type, UInt64Type, UInt8Type,
};
use arrow_array::{Array, Float32Array, RecordBatch};
use lexical_util::num::AsPrimitive;
use snowflake_api::{QueryResult, SnowflakeApi};
use std::any::Any;
use std::collections::HashMap;
use tracing::info;

pub type Row = HashMap<String, Box<dyn Any + Send>>;

#[async_trait::async_trait]
pub trait QueryRunner: Send + Sync + 'static {
    /// Asynchronously runs the given SQL query and returns the result as a floating-point number.
    /// Only the first row and first column in returned, so it is important that users of this trait
    /// use it for a query which returns only a single floating point result
    async fn run_single_entry(&self, query: &str) -> anyhow::Result<f64>;

    /// Asynchronously runs the given SQL query and returns the result as a vector of rows.
    async fn run(&self, query: &str) -> anyhow::Result<Vec<Row>>;
}

macro_rules! insert_primitive_values {
    ($rows:expr, $column:expr, $name:expr, $type:ty) => {
        if let Some(value) = $column.as_primitive_opt::<$type>() {
            for i in 0..value.len() {
                let entry = $rows.get_mut(i);
                if let Some(entry) = entry {
                    entry.insert($name.clone(), Box::new(value.value(i)));
                } else {
                    $rows.push(HashMap::new());
                    $rows
                        .last_mut()
                        .unwrap()
                        .insert($name.clone(), Box::new(value.value(i)));
                }
            }
            continue;
        }
    };
}

macro_rules! insert_string_values {
    ($rows:expr, $column:expr, $name:expr, $type:ty) => {
        if let Some(value) = $column.as_string_opt::<$type>() {
            for i in 0..value.len() {
                let entry = $rows.get_mut(i);
                if let Some(entry) = entry {
                    entry.insert($name.clone(), Box::new(value.value(i).to_string()));
                } else {
                    $rows.push(HashMap::new());
                    $rows
                        .last_mut()
                        .unwrap()
                        .insert($name.clone(), Box::new(value.value(i).to_string()));
                }
            }
            continue;
        }
    };
}

pub struct SnowflakeQueryRunner {
    account_identifier: String,
    warehouse: String,
    database: String,
    schema: String,
    user: String,
    role: String,
    passwd: String,
}

impl SnowflakeQueryRunner {
    /// Creates a new `SnowflakeQueryRunner` with the specified connection parameters.
    ///
    /// # Arguments
    /// * `account_identifier` - Snowflake account identifier.
    /// * `warehouse` - The Snowflake warehouse to use.
    /// * `database` - The database to query against.
    /// * `schema` - The schema within the database.
    /// * `user` - Username for authentication.
    /// * `role` - User role for executing queries.
    /// * `passwd` - Password for authentication.
    pub fn new(
        account_identifier: &str,
        warehouse: &str,
        database: &str,
        schema: &str,
        user: &str,
        role: &str,
        passwd: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            account_identifier: account_identifier.to_string(),
            warehouse: warehouse.to_string(),
            database: database.to_string(),
            schema: schema.to_string(),
            user: user.to_string(),
            role: role.to_string(),
            passwd: passwd.to_string(),
        })
    }

    pub fn from_config(
        config: &SecurityWatchdogConfig,
        sf_password: String,
    ) -> anyhow::Result<Self> {
        Self::new(
            config
                .sf_account_identifier
                .as_ref()
                .cloned()
                .unwrap()
                .as_str(),
            config.sf_warehouse.as_ref().cloned().unwrap().as_str(),
            config.sf_database.as_ref().cloned().unwrap().as_str(),
            config.sf_schema.as_ref().cloned().unwrap().as_str(),
            config.sf_username.as_ref().cloned().unwrap().as_str(),
            config.sf_role.as_ref().cloned().unwrap().as_str(),
            sf_password.clone().as_str(),
        )
    }

    pub fn make_snowflake_api(&self) -> anyhow::Result<SnowflakeApi> {
        let api = SnowflakeApi::with_password_auth(
            &self.account_identifier,
            Some(&self.warehouse),
            Some(&self.database),
            Some(&self.schema),
            &self.user,
            Some(&self.role),
            &self.passwd,
        )?;
        Ok(api)
    }

    /// Parses the result of a Snowflake query from a `Vec<RecordBatch>` into a single `f64` value.
    fn parse(&self, res: Vec<RecordBatch>) -> anyhow::Result<f64> {
        let value = res
            .first()
            .ok_or_else(|| anyhow!("No results found in RecordBatch"))?
            .columns()
            .first()
            .ok_or_else(|| anyhow!("No columns found in record"))?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or_else(|| anyhow!("Column is not Float32Array"))?
            .value(0)
            .as_f64();
        Ok(value)
    }

    fn parse_record_batch(&self, batch: RecordBatch) -> anyhow::Result<Vec<Row>> {
        let mut rows: Vec<Row> = Vec::new();
        for (index, column) in batch.columns().iter().enumerate() {
            let name = batch.schema().fields()[index].name().clone();
            insert_primitive_values!(rows, column, name, Int8Type);
            insert_primitive_values!(rows, column, name, Int16Type);
            insert_primitive_values!(rows, column, name, Int32Type);
            insert_primitive_values!(rows, column, name, Int64Type);
            insert_primitive_values!(rows, column, name, UInt8Type);
            insert_primitive_values!(rows, column, name, UInt16Type);
            insert_primitive_values!(rows, column, name, UInt32Type);
            insert_primitive_values!(rows, column, name, UInt64Type);
            insert_primitive_values!(rows, column, name, Float16Type);
            insert_primitive_values!(rows, column, name, Float32Type);
            insert_primitive_values!(rows, column, name, Float64Type);
            insert_primitive_values!(rows, column, name, Decimal128Type);
            insert_string_values!(rows, column, name, i32);
            insert_string_values!(rows, column, name, i64);
            let schema = batch.schema();
            let data_type = schema.fields()[index].data_type();
            let metadata = schema.fields()[index].metadata();
            info!(
                "Skipping column: {}, data_type: {:?}, metadata: {:?}",
                name, data_type, metadata
            );
        }
        Ok(rows)
    }

    fn parse_record_batches(&self, batches: Vec<RecordBatch>) -> anyhow::Result<Vec<Row>> {
        let mut rows: Vec<Row> = Vec::new();
        for batch in batches {
            let mut batch_rows = self.parse_record_batch(batch)?;
            rows.append(&mut batch_rows);
        }
        info!("Found {} rows", rows.len());
        Ok(rows)
    }
}

#[async_trait::async_trait]
impl QueryRunner for SnowflakeQueryRunner {
    async fn run_single_entry(&self, query: &str) -> anyhow::Result<f64> {
        let res = self.make_snowflake_api()?.exec(query).await?;
        match res {
            QueryResult::Arrow(records) => self.parse(records),
            // Handle other result types (Json, Empty) with a unified error message
            _ => Err(anyhow!("Unexpected query result type")),
        }
    }

    async fn run(&self, query: &str) -> anyhow::Result<Vec<Row>> {
        info!("Running query: {}", query);
        let res = self.make_snowflake_api()?.exec(query).await?;
        match res {
            QueryResult::Arrow(records) => self.parse_record_batches(records),
            QueryResult::Empty => Ok(Vec::new()),
            // Handle other result types Json with a unified error message
            _ => Err(anyhow!("Unexpected query result type")),
        }
    }
}
