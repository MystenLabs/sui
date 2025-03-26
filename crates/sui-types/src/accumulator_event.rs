// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::digests::Digest;
use move_core_types::language_storage::StructTag;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccumulatorAction {
    Merge,
    Split,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccumulatorValue {
    MoveValue(StructTag, Vec<u8>),
    EventCommitment(Digest),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccumulatorEvent {
    pub action: AccumulatorAction,
    pub target: SuiAddress,
    pub value: AccumulatorValue,
}

impl AccumulatorEvent {
    pub fn new(action: AccumulatorAction, target: SuiAddress, value: AccumulatorValue) -> Self {
        Self {
            action,
            target,
            value,
        }
    }
}
