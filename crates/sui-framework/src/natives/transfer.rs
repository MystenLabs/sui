// Copyright (c) 2022, Mysten Labs, Inc.
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
/// `transfer_internal<T: key>(obj: T, recipient: vector<u8>, to_object: bool)`
/// Here, we simply emit this event. The sui adapter
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
    let to_object = pop_arg!(args, bool);
    let recipient = pop_arg!(args, AccountAddress);
    let transferred_obj = args.pop_back().unwrap();
    let event_type = if to_object {
        EventType::TransferToObject
    } else {
        EventType::TransferToAddress
    };
    // Charge a constant native gas cost here, since
    // we will charge it properly when processing
    // all the events in adapter.
    // TODO: adjust native_gas cost size base.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if context.save_event(recipient.to_vec(), event_type as u64, ty, transferred_obj)? {
        Ok(NativeResult::ok(cost, smallvec![]))
    } else {
        Ok(NativeResult::err(cost, 0))
    }
}

/// Implementation of Move native function
/// `freeze_object<T: key>(obj: T)`
pub fn freeze_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();
    let event_type = EventType::FreezeObject;
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if context.save_event(vec![], event_type as u64, ty, obj)? {
        Ok(NativeResult::ok(cost, smallvec![]))
    } else {
        Ok(NativeResult::err(cost, 0))
    }
}

/// Implementation of Move native function
/// `share_object<T: key>(obj: T)`
pub fn share_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();
    let event_type = EventType::ShareObject;
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if context.save_event(vec![], event_type as u64, ty, obj)? {
        Ok(NativeResult::ok(cost, smallvec![]))
    } else {
        Ok(NativeResult::err(cost, 0))
    }
}

/// Implementation of Move native function
/// `delete_child_object_internal<T: key>(child: T)`
pub fn delete_child_object_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    // first args is an object ID that we will emit in a DeleteChildObject event
    // second arg is VersionedID that we want to ignore
    debug_assert!(args.len() == 2);

    let obj_id = args.pop_front().unwrap();
    let event_type = EventType::DeleteChildObject;
    // TODO: Decide the cost.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 1);
    if context.save_event(vec![], event_type as u64, Type::Address, obj_id)? {
        Ok(NativeResult::ok(cost, smallvec![]))
    } else {
        Ok(NativeResult::err(cost, 0))
    }
}
