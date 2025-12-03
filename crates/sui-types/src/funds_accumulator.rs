// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::funds_accumulator`

use crate::SUI_FRAMEWORK_ADDRESS;
use crate::base_types::SuiAddress;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::u256::U256;
use serde::Deserialize;
use serde::Serialize;

pub const FUNDS_ACCUMULATOR_MODULE_NAME: &IdentStr = ident_str!("funds_accumulator");
pub const WITHDRAWAL_STRUCT_NAME: &IdentStr = ident_str!("Withdrawal");
pub const RESOLVED_WITHDRAWAL_STRUCT: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    FUNDS_ACCUMULATOR_MODULE_NAME,
    WITHDRAWAL_STRUCT_NAME,
);

/// Rust bindings for the Move struct `sui::funds_accumulator::Withdrawal`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Withdrawal {
    pub owner: SuiAddress,
    /// Note that unlike the `CallArg::FundsWithdrawal` the `limit` here must
    /// be fully specified, and cannot be `None` (i.e., unlimited).
    /// As such, it is the responsibility of the PTB runtime to determine
    /// the maximum limit in such a case, before creating the Move value.
    pub limit: U256,
}

impl Withdrawal {
    pub fn new(owner: SuiAddress, limit: U256) -> Self {
        Self { owner, limit }
    }

    pub fn type_(type_arg: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: FUNDS_ACCUMULATOR_MODULE_NAME.to_owned(),
            name: WITHDRAWAL_STRUCT_NAME.to_owned(),
            type_params: vec![type_arg],
        }
    }

    pub fn type_tag(type_param: TypeTag) -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_(type_param)))
    }

    pub fn is_withdrawal(s: &StructTag) -> bool {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == FUNDS_ACCUMULATOR_MODULE_NAME
            && s.name.as_ident_str() == WITHDRAWAL_STRUCT_NAME
            && s.type_params.len() == 1
    }

    pub fn is_withdrawal_type(type_param: &TypeTag) -> bool {
        if let TypeTag::Struct(struct_tag) = type_param {
            Self::is_withdrawal(struct_tag)
        } else {
            false
        }
    }

    pub fn owner(&self) -> SuiAddress {
        self.owner
    }

    pub fn limit(&self) -> U256 {
        self.limit
    }

    pub fn layout(type_param: TypeTag) -> MoveStructLayout {
        MoveStructLayout {
            type_: Self::type_(type_param),
            fields: vec![
                MoveFieldLayout::new(ident_str!("owner").to_owned(), MoveTypeLayout::Address),
                MoveFieldLayout::new(ident_str!("limit").to_owned(), MoveTypeLayout::U256),
            ],
        }
    }
}
