// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use mysten_common::fatal;

use crate::accumulator_root::AccumulatorObjId;
use crate::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
};
use crate::gas_coin::GasCoin;
use crate::TypeTag;

pub const ACCUMULATOR_MODULE_NAME: &IdentStr = ident_str!("accumulator");

#[derive(Debug, Clone)]
pub struct AccumulatorEvent {
    pub accumulator_obj: AccumulatorObjId,
    pub write: AccumulatorWriteV1,
}

impl AccumulatorEvent {
    pub fn new(accumulator_obj: AccumulatorObjId, write: AccumulatorWriteV1) -> Self {
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

        let sui = match ty {
            TypeTag::Struct(struct_tag) => {
                if !GasCoin::is_gas_balance(struct_tag) {
                    0
                } else {
                    match value {
                        AccumulatorValue::Integer(v) => *v,
                        AccumulatorValue::IntegerTuple(_, _) => fatal!("invalid accumulator value"),
                    }
                }
            }
            _ => 0,
        };

        match operation {
            AccumulatorOperation::Merge => (0, sui),
            AccumulatorOperation::Split => (sui, 0),
        }
    }
}
