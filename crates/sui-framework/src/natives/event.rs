// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_emit_cost, EventType};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

/// Implementation of Move native function `event::emit<T: copy + drop>(event: T)`
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
    let cost = legacy_emit_cost();
    match ty {
        Type::Struct(..) | Type::StructInstantiation(..) => (),
        ty => {
            // TODO (https://github.com/MystenLabs/sui/issues/19): ideally enforce this in the ability system
            return Err(PartialVMError::new(StatusCode::DATA_FORMAT_ERROR)
                .with_message(format!("Unsupported event type {:?} (struct expected)", ty)));
        }
    }

    if !context.save_event(Vec::new(), EventType::User as u64, ty, event)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
