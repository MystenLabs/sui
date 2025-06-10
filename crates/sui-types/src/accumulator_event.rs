// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

use crate::base_types::ObjectID;
use crate::effects::AccumulatorWriteV1;

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
}
