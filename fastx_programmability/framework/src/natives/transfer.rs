// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

/// Implementation of Move native function
/// `transfer_internal<T: key>(event: TransferEvent<T>)`
/// Here, we simply emit this event. The fastX adapter
/// treats this as a special event that is handled
/// differently from user events--for each `TransferEvent`,
/// the adapter will change the owner of the object
/// in question to `TransferEvent.recipient`
pub fn transfer_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    let ty = ty_args.pop().unwrap();
    let should_freeze = pop_arg!(args, bool);
    let recipient = pop_arg!(args, Vec<u8>);
    let transferred_obj = args.pop_back().unwrap();

    // Charge a constant native gas cost here, since
    // we will charge it properly when processing
    // all the events in adapter.
    // TODO: adjust native_gas cost size base.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if !context.save_event(recipient, should_freeze as u64, ty, transferred_obj)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
