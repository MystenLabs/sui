// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{ExecutionError, ExecutionErrorKind};
use crate::SUI_FRAMEWORK_ADDRESS;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
pub const BALANCE_MODULE_NAME: &IdentStr = ident_str!("balance");
pub const BALANCE_STRUCT_NAME: &IdentStr = ident_str!("Balance");

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Supply {
    pub value: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Eq, PartialEq)]
pub struct Balance {
    value: u64,
}

impl Balance {
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: BALANCE_MODULE_NAME.to_owned(),
            name: BALANCE_STRUCT_NAME.to_owned(),
            type_params: vec![TypeTag::Struct(type_param)],
        }
    }

    pub fn withdraw(&mut self, amount: u64) -> Result<(), ExecutionError> {
        fp_ensure!(
            self.value >= amount,
            ExecutionError::new_with_source(
                ExecutionErrorKind::TransferInsufficientBalance,
                format!("balance: {} required: {}", self.value, amount)
            )
        );
        self.value -= amount;
        Ok(())
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout(type_param: StructTag) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(type_param),
            fields: vec![MoveFieldLayout::new(
                ident_str!("value").to_owned(),
                MoveTypeLayout::U64,
            )],
        }
    }
}
