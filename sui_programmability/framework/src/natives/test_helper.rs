// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
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

pub fn get_last_received_object(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let signer = pop_arg!(args, Vec<u8>);

    // Gas amount doesn't matter as this is test only.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);

    let mut transfer_events =
        context
            .events()
            .iter()
            .rev()
            .filter(|(recipient, event_type, _, _, _)| {
                (*event_type == EventType::TransferToAddress as u64
                    || *event_type == EventType::TransferToAddressAndFreeze as u64)
                    && recipient == &signer
            });
    let latest_event = transfer_events.next();
    match latest_event {
        Some((_, _, _, _, obj)) => Ok(NativeResult::ok(cost, smallvec![obj.copy_value()?])),
        None => Ok(NativeResult::err(cost, 0)),
    }
}
