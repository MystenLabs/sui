// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{ev_emit_mod, ev_struct_inst};
use diesel::prelude::*;
use sui_field_count::FieldCount;

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, FieldCount, Queryable)]
#[diesel(table_name = ev_emit_mod)]
pub struct StoredEvEmitMod {
    pub package: Vec<u8>,
    pub module: String,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, FieldCount, Queryable)]
#[diesel(table_name = ev_struct_inst)]
pub struct StoredEvStructInst {
    pub package: Vec<u8>,
    pub module: String,
    pub name: String,
    pub instantiation: Vec<u8>,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}
