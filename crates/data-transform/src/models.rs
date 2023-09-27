// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::events_json;
use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::events)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Event {
    #[diesel(deserialize_as = i64)]
    pub id: i64,
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub sender: String,
    pub package: String,
    pub module: String,
    pub event_type: String,
    pub event_time_ms: Option<i64>,
    pub event_bcs: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = events_json)]
pub struct EventsJson {
    pub id: i64,
    pub event_json: String,
}
