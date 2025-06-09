// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::effects::AccumulatorWriteV1;

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
