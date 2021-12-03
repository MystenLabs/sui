// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_schedule::GasAlgebra};
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
    debug_assert!(args.len() == 2);

    let ty = ty_args.pop().unwrap();
    let recipient = pop_arg!(args, AccountAddress);
    let transferred_obj = args.pop_back().unwrap();

    // Charge by size of transferred object
    let cost = native_gas(
        context.cost_table(),
        NativeCostIndex::EMIT_EVENT,
        transferred_obj.size().get() as usize,
    );
    let seq_num = 0;
    if !context.save_event(recipient.to_vec(), seq_num, ty, transferred_obj)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
