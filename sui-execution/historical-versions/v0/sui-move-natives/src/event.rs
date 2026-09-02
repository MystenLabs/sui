// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{gas_algebra::InternalGas, language_storage::TypeTag, vm_status::StatusCode};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::error::VMMemoryLimitExceededSubStatusCode;

#[derive(Clone, Debug)]
pub struct EventEmitCostParams {
    pub event_emit_cost_base: InternalGas,
    pub event_emit_value_size_derivation_cost_per_byte: InternalGas,
    pub event_emit_tag_size_derivation_cost_per_byte: InternalGas,
    pub event_emit_output_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun emit
 * Implementation of the Move native function `event::emit<T: copy + drop>(event: T)`
 * Adds an event to the transaction's event log
 *   gas cost: event_emit_cost_base                  |  covers various fixed costs in the oper
 *              + event_emit_value_size_derivation_cost_per_byte * event_size     | derivation of size
 *              + event_emit_tag_size_derivation_cost_per_byte * tag_size         | converting type
 *              + event_emit_output_cost_per_byte * (tag_size + event_size)       | emitting the actual event
 **************************************************************************************************/
pub fn emit(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let event_emit_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .event_emit_cost_params
        .clone();

    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let ty = ty_args.pop().unwrap();
    let event_value = args.pop_back().unwrap();

    let event_value_size = event_value.legacy_size();

    // Deriving event value size can be expensive due to recursion overhead
    native_charge_gas_early_exit!(
        context,
        event_emit_cost_params.event_emit_value_size_derivation_cost_per_byte
            * u64::from(event_value_size).into()
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
        event_emit_cost_params.event_emit_tag_size_derivation_cost_per_byte
            * u64::from(tag_size).into()
    );

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let max_event_emit_size = obj_runtime.constants.max_event_emit_size;
    let ev_size = u64::from(tag_size + event_value_size);
    if ev_size > max_event_emit_size {
        return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
            .with_message(format!(
                "Emitting event of size {ev_size} bytes. Limit is {max_event_emit_size} bytes."
            ))
            .with_sub_status(
                VMMemoryLimitExceededSubStatusCode::EVENT_SIZE_LIMIT_EXCEEDED as u64,
            ));
    }

    // Emitting an event is cheap since its a vector push
    native_charge_gas_early_exit!(
        context,
        event_emit_cost_params.event_emit_output_cost_per_byte * ev_size.into()
    );

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();

    obj_runtime.emit_event(ty, *tag, event_value)?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
