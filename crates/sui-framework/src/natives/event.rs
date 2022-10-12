// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_emit_cost, natives::object_runtime::ObjectRuntime};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{language_storage::TypeTag, vm_status::StatusCode};
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
    let tag = match context.type_to_type_tag(&ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.emit_event(ty, tag, event);
    Ok(NativeResult::ok(cost, smallvec![]))
}
