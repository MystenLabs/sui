// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::natives::object_runtime::ObjectRuntime;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{language_storage::TypeTag, vm_status::StatusCode};
use move_vm_runtime::{
    native_charge_gas_early_exit, native_functions::NativeContext, native_gas_total_cost,
};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, ops::Mul};
use sui_cost_tables::natives_tables::NATIVES_COST_MID;
use sui_types::error::VMMemoryLimitExceededSubStatusCode;

/***************************************************************************************************
 * native fun to_u256
 * Implementation of the Move native function `event::emit<T: copy + drop>(event: T)`
 * Adds an event to the transaction's event log
 *   gas cost: NATIVES_COST_MID * event_size                      | derivation of size
 *              + NATIVES_COST_MID * tag_size                     | converting type
 *              + NATIVES_COST_MID * (tag_size + event_size)      | emitting the actual event
 **************************************************************************************************/
pub fn emit(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);
    let mut gas_left = context.gas_budget();

    let ty = ty_args.pop().unwrap();
    let event = args.pop_back().unwrap();

    let event_size = event.legacy_size();

    // Deriving event size can be expensive due to recursion overhead
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_MID.mul(u64::from(event_size).into())
    );

    let tag = match context.type_to_type_tag(&ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let tag_size = tag.abstract_size_for_gas_metering();

    // Converting type to typetag be expensive due to recursion overhead
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_MID.mul(u64::from(tag_size).into())
    );

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let max_event_emit_size = obj_runtime.constants.max_event_emit_size;
    let ev_size = u64::from(tag_size + event_size);
    if ev_size > max_event_emit_size {
        return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
            .with_message(format!(
                "Emitting event of size {ev_size} bytes. Limit is {max_event_emit_size} bytes."
            ))
            .with_sub_status(
                VMMemoryLimitExceededSubStatusCode::EVENT_SIZE_LIMIT_EXCEEDED as u64,
            ));
    }
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();

    // Emitting an event is cheap since its a vector push
    native_charge_gas_early_exit!(context, gas_left, NATIVES_COST_MID.mul(ev_size.into()));
    obj_runtime.emit_event(*tag, event)?;
    Ok(NativeResult::ok(
        native_gas_total_cost!(context, gas_left),
        smallvec![],
    ))
}
