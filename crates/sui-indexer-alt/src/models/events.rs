// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{
    ev_emit_mod, ev_emit_pkg, ev_struct_inst, ev_struct_mod, ev_struct_name, ev_struct_pkg,
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use sui_types::TypeTag;

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_emit_mod)]
pub struct StoredEvEmitMod {
    pub package: Vec<u8>,
    pub module: String,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_emit_pkg)]
pub struct StoredEvEmitPkg {
    pub package: Vec<u8>,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_struct_inst)]
pub struct StoredEvStructInst {
    pub package: Vec<u8>,
    pub module: String,
    pub name: String,
    pub instantiation: Vec<u8>,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_struct_mod)]
pub struct StoredEvStructMod {
    pub package: Vec<u8>,
    pub module: String,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_struct_name)]
pub struct StoredEvStructName {
    pub package: Vec<u8>,
    pub module: String,
    pub name: String,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[diesel(table_name = ev_struct_pkg)]
pub struct StoredEvStructPkg {
    pub package: Vec<u8>,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

/// This is the deserialized form of [StoredEvStructInst::instantiation], which is stored as BCS.
#[derive(Serialize, Deserialize, Debug)]
pub struct StructInstantiation {
    pub name: String,
    pub type_params: Vec<TypeTag>,
}
