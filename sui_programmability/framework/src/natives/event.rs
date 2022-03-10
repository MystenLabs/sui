// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_schedule::GasAlgebra;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

/// Implementation of Move native function `Event::emit<T: copy + drop>(event: T)`
/// Adds an event to the transaction's event log
pub fn emit(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let ty = ty_args.pop().unwrap();
    let event = args.pop_back().unwrap();

    // gas cost is proportional to size of event
    let event_size = event.size();
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1).add(event_size);
    match ty {
        Type::Struct(..) | Type::StructInstantiation(..) => (),
        ty => {
            // TODO: // TODO(https://github.com/MystenLabs/sui/issues/19): enforce this in the ability system
            panic!("Unsupported event type {:?}--struct expected", ty)
        }
    }

    if !context.save_event(Vec::new(), EventType::User as u64, ty, event)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
