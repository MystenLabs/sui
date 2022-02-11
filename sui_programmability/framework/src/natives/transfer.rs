// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
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
/// `transfer_internal<T: key>(obj: T, recipient: vector<u8>, should_freeze: bool)`
/// Here, we simply emit this event. The fastX adapter
/// treats this as a special event that is handled
/// differently from user events:
/// the adapter will change the owner of the object
/// in question to `recipient`.
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
    let event_type = if should_freeze {
        EventType::TransferToAddressAndFreeze
    } else {
        EventType::TransferToAddress
    };
    transfer_common(context, ty, transferred_obj, recipient, event_type)
}

/// Implementation of Move native function
/// `transfer_to_object_id<T: key>(obj: T, id: IDBytes)`
pub fn transfer_to_object_id(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 2);

    let ty = ty_args.pop().unwrap();
    let recipient = pop_arg!(args, AccountAddress).to_vec();
    let transferred_obj = args.pop_back().unwrap();
    let event_type = EventType::TransferToObject;
    transfer_common(context, ty, transferred_obj, recipient, event_type)
}

fn transfer_common(
    context: &mut NativeContext,
    ty: Type,
    transferred_obj: Value,
    recipient: Vec<u8>,
    event_type: EventType,
) -> PartialVMResult<NativeResult> {
    // Charge a constant native gas cost here, since
    // we will charge it properly when processing
    // all the events in adapter.
    // TODO: adjust native_gas cost size base.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if !context.save_event(recipient, event_type as u64, ty, transferred_obj)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
