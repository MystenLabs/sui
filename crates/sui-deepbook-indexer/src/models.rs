// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{deep_price, deepbook, progress_store};
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use diesel::data_types::PgTimestamp;

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = progress_store, primary_key(task_name))]
pub struct ProgressStore {
    pub task_name: String,
    pub checkpoint: i64,
    pub target_checkpoint: i64,
    pub timestamp: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = deepbook, primary_key(digest))]
pub struct Deepbook {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = deep_price, primary_key(digest))]
pub struct DeepPrice {
    pub digest: String,
    pub sender: String,
    pub target_pool: String,
    pub reference_pool: String,
    pub checkpoint: i64,
    pub timestamp: PgTimestamp,
}

pub enum DeepbookType {
    Deepbook(Deepbook),
    DeepPrice(DeepPrice),
}