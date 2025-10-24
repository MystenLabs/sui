// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SUI_FRAMEWORK_ADDRESS;
use crate::error::{ExecutionError, ExecutionErrorKind};
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use move_core_types::annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
pub const SUI_MODULE_NAME: &IdentStr = ident_str!("sui");
pub const BALANCE_MODULE_NAME: &IdentStr = ident_str!("balance");
pub const BALANCE_STRUCT_NAME: &IdentStr = ident_str!("Balance");
pub const BALANCE_CREATE_REWARDS_FUNCTION_NAME: &IdentStr = ident_str!("create_staking_rewards");
pub const BALANCE_DESTROY_REBATES_FUNCTION_NAME: &IdentStr = ident_str!("destroy_storage_rebates");

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Supply {
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
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

    pub fn type_(type_param: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: BALANCE_MODULE_NAME.to_owned(),
            name: BALANCE_STRUCT_NAME.to_owned(),
            type_params: vec![type_param],
        }
    }

    pub fn type_tag(type_param: TypeTag) -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_(type_param)))
    }

    pub fn is_balance(s: &StructTag) -> bool {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == BALANCE_MODULE_NAME
            && s.name.as_ident_str() == BALANCE_STRUCT_NAME
    }

    pub fn is_balance_type(type_param: &TypeTag) -> bool {
        if let TypeTag::Struct(struct_tag) = type_param {
            Self::is_balance(struct_tag)
        } else {
            false
        }
    }

    /// If the given type is `Balance<T>`, return `Some(T)`.
    pub fn maybe_get_balance_type_param(ty: &TypeTag) -> Option<TypeTag> {
        if let TypeTag::Struct(struct_tag) = ty
            && Self::is_balance(struct_tag)
        {
            assert_eq!(struct_tag.type_params.len(), 1);
            return Some(struct_tag.type_params[0].clone());
        }
        None
    }

    pub fn withdraw(&mut self, amount: u64) -> Result<(), ExecutionError> {
        fp_ensure!(
            self.value >= amount,
            ExecutionError::new_with_source(
                ExecutionErrorKind::InsufficientCoinBalance,
                format!("balance: {} required: {}", self.value, amount)
            )
        );
        self.value -= amount;
        Ok(())
    }

    pub fn deposit_for_safe_mode(&mut self, amount: u64) {
        self.value += amount;
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout(type_param: TypeTag) -> MoveStructLayout {
        MoveStructLayout {
            type_: Self::type_(type_param),
            fields: vec![MoveFieldLayout::new(
                ident_str!("value").to_owned(),
                MoveTypeLayout::U64,
            )],
        }
    }

    /// Check if a struct layout represents a `Balance<T>` type with the expected field structure.
    pub fn is_balance_layout(struct_layout: &MoveStructLayout) -> bool {
        let ty = &struct_layout.type_;

        if !Self::is_balance(ty) {
            return false;
        }

        if ty.type_params.len() != 1 {
            return false;
        }

        if struct_layout.fields.len() != 1 {
            return false;
        }

        let Some(field) = struct_layout.fields.first() else {
            return false;
        };

        if field.name.as_str() != "value" {
            return false;
        }

        if !matches!(field.layout, MoveTypeLayout::U64) {
            return false;
        }

        true
    }
}
