// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::{Deserialize, Serialize};

use crate::{id::UID, SUI_FRAMEWORK_ADDRESS};

pub const CLOCK_MODULE_NAME: &IdentStr = ident_str!("clock");
pub const CLOCK_STRUCT_NAME: &IdentStr = ident_str!("Clock");
pub const CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME: &IdentStr =
    ident_str!("consensus_commit_prologue");

#[derive(Debug, Serialize, Deserialize)]
pub struct Clock {
    pub id: UID,
    pub timestamp_ms: u64,
}

impl Clock {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: CLOCK_MODULE_NAME.to_owned(),
            name: CLOCK_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}
