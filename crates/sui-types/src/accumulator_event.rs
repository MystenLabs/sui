// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use mysten_common::fatal;

use crate::balance::Balance;
use crate::base_types::ObjectID;
use crate::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
};
use crate::gas_coin::GAS;
use crate::TypeTag;

pub const ACCUMULATOR_MODULE_NAME: &IdentStr = ident_str!("accumulator");

#[derive(Debug, Clone)]
pub struct AccumulatorEvent {
    pub accumulator_obj: ObjectID,
    pub write: AccumulatorWriteV1,
}

impl AccumulatorEvent {
    pub fn new(accumulator_obj: ObjectID, write: AccumulatorWriteV1) -> Self {
        Self {
            accumulator_obj,
            write,
        }
    }

    pub fn total_sui_in_event(&self) -> (u64 /* input */, u64 /* output */) {
        let Self {
            write:
                AccumulatorWriteV1 {
                    address: AccumulatorAddress { ty, .. },
                    operation,
                    value,
                },
            ..
        } = self;

        let TypeTag::Struct(struct_ty) = ty else {
            return (0, 0);
        };

        if !Balance::is_balance(struct_ty) {
            return (0, 0);
        }

        debug_assert!(struct_ty.type_params.len() == 1);
        let TypeTag::Struct(coin_type) = &struct_ty.type_params[0] else {
            // T is not a struct type
            return (0, 0);
        };

        if !GAS::is_gas(coin_type) {
            return (0, 0);
        }

        debug_assert_eq!(
            *ty,
            "0x2::balance::Balance<0x2::sui::SUI>"
                .parse::<TypeTag>()
                .unwrap()
        );

        let AccumulatorValue::Integer(value) = value else {
            fatal!("Balance<SUI> accumulator value is not an integer");
        };

        match operation {
            AccumulatorOperation::Merge => (0, *value),
            AccumulatorOperation::Split => (*value, 0),
        }
    }
}
