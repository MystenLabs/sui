// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Binary, Text};
use diesel::QueryableByName;

use move_core_types::identifier::Identifier;
use sui_json_rpc_types::MoveFunctionName;
use sui_types::base_types::ObjectID;

use crate::errors::IndexerError;
use crate::schema::{move_call_metrics, move_calls};

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = move_calls)]
pub struct StoredMoveCall {
    pub transaction_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub move_package: Vec<u8>,
    pub move_module: String,
    pub move_function: String,
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = move_call_metrics)]
pub struct StoredMoveCallMetrics {
    pub id: Option<i64>,
    pub epoch: i64,
    pub day: i64,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
    pub count: i64,
}

impl Default for StoredMoveCallMetrics {
    fn default() -> Self {
        Self {
            id: None,
            epoch: -1,
            day: -1,
            move_package: "".to_string(),
            move_module: "".to_string(),
            move_function: "".to_string(),
            count: -1,
        }
    }
}

// for auto-incremented id, the committed id is None, so Option<i64>,
// but when querying, the returned type is i64, thus a separate type is needed.
#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = move_call_metrics)]
pub struct QueriedMoveCallMetrics {
    pub id: i64,
    pub epoch: i64,
    pub day: i64,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
    pub count: i64,
}

impl TryInto<(MoveFunctionName, usize)> for QueriedMoveCallMetrics {
    type Error = IndexerError;

    fn try_into(self) -> Result<(MoveFunctionName, usize), Self::Error> {
        let package = ObjectID::from_str(&self.move_package)?;
        let module = Identifier::from_str(&self.move_module)?;
        let function = Identifier::from_str(&self.move_function)?;
        Ok((
            MoveFunctionName {
                package,
                module,
                function,
            },
            self.count as usize,
        ))
    }
}

impl From<QueriedMoveCallMetrics> for StoredMoveCallMetrics {
    fn from(q: QueriedMoveCallMetrics) -> Self {
        StoredMoveCallMetrics {
            id: Some(q.id),
            epoch: q.epoch,
            day: q.day,
            move_package: q.move_package,
            move_module: q.move_module,
            move_function: q.move_function,
            count: q.count,
        }
    }
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct QueriedMoveMetrics {
    #[diesel(sql_type = BigInt)]
    pub epoch: i64,
    #[diesel(sql_type = BigInt)]
    pub day: i64,
    #[diesel(sql_type = Binary)]
    pub move_package: Vec<u8>,
    #[diesel(sql_type = Text)]
    pub move_module: String,
    #[diesel(sql_type = Text)]
    pub move_function: String,
    #[diesel(sql_type = BigInt)]
    pub count: i64,
}

pub fn build_move_call_metric_query(epoch: i64, days: i64) -> String {
    format!("SELECT {}::BIGINT AS epoch, {}::BIGINT AS day, move_package, move_module, move_function, COUNT(*)::BIGINT AS count
        FROM move_calls
        WHERE epoch >= {}
        GROUP BY move_package, move_module, move_function
        ORDER BY count DESC
        LIMIT 10;", epoch, days, epoch - days)
}
