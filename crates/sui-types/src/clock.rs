// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{binary_views::BinaryIndexedView, file_format::SignatureToken};
use move_bytecode_utils::resolve_struct;
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::StructTag,
};
use serde::{Deserialize, Serialize};

use crate::{id::UID, SUI_FRAMEWORK_ADDRESS};

pub const CLOCK_MODULE_NAME: &IdentStr = ident_str!("clock");
pub const CLOCK_STRUCT_NAME: &IdentStr = ident_str!("Clock");
pub const RESOLVED_SUI_CLOCK: (&AccountAddress, &IdentStr, &IdentStr) =
    (&SUI_FRAMEWORK_ADDRESS, CLOCK_MODULE_NAME, CLOCK_STRUCT_NAME);
pub const CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME: &IdentStr =
    ident_str!("consensus_commit_prologue");

#[derive(Debug, Serialize, Deserialize)]
pub struct Clock {
    pub id: UID,
    pub timestamp_ms: u64,
}

impl Clock {
    pub fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: CLOCK_MODULE_NAME.to_owned(),
            name: CLOCK_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Detects a `&mut sui::clock::Clock` or `sui::clock::Clock` in the signature.
    pub fn is_mutable(view: &BinaryIndexedView<'_>, s: &SignatureToken) -> bool {
        use SignatureToken as S;
        match s {
            S::MutableReference(inner) => Self::is_mutable(view, inner),
            S::Datatype(idx) => resolve_struct(view, *idx) == RESOLVED_SUI_CLOCK,
            _ => false,
        }
    }
}
