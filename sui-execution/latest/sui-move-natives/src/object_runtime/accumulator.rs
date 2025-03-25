// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};

pub enum AccumulatorAction {
    Merge,
    Split,
}

pub enum AccumulatorValue {
    MoveValue(Type, StructTag, Value),
    // commit the nth event emitted by the transaction to an event stream
    EventRef(u64),
}

pub struct AccumulatorEvent {
    pub action: AccumulatorAction,
    pub target: AccountAddress,
    pub value: AccumulatorValue,
}
