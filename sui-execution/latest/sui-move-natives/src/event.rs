// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    abstract_size, get_extension, get_extension_mut, legacy_test_cost,
    object_runtime::{MoveAccumulatorAction, MoveAccumulatorValue, ObjectRuntime},
    NativesCostTable,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, language_storage::TypeTag,
    vm_status::StatusCode,
};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Value, VectorSpecialization},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{base_types::ObjectID, error::VMMemoryLimitExceededSubStatusCode};

pub const NOT_SUPPORTED: u64 = 0;

#[derive(Clone, Debug)]
pub struct EventEmitCostParams {
    pub event_emit_cost_base: InternalGas,
    pub event_emit_value_size_derivation_cost_per_byte: InternalGas,
    pub event_emit_tag_size_derivation_cost_per_byte: InternalGas,
    pub event_emit_output_cost_per_byte: InternalGas,
    pub event_emit_auth_stream_cost: Option<InternalGas>,
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

    let ty = ty_args.pop().unwrap();
    let event_value = args.pop_back().unwrap();
    emit_impl(context, ty, event_value, None)
}

pub fn emit_authenticated_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 2);
    debug_assert!(args.len() == 3);

    let cost = context.gas_used();
    if !get_extension!(context, ObjectRuntime)?
        .protocol_config
        .enable_authenticated_event_streams()
    {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED));
    }

    let event_ty = ty_args.pop().unwrap();
    // This type is always sui::event::EventStreamHead
    let stream_head_ty = ty_args.pop().unwrap();

    let event_value = args.pop_back().unwrap();
    let stream_id = args.pop_back().unwrap();
    let accumulator_id = args.pop_back().unwrap();

    emit_impl(
        context,
        event_ty,
        event_value,
        Some(StreamRef {
            accumulator_id,
            stream_id,
            stream_head_ty,
        }),
    )
}

struct StreamRef {
    // The pre-computed id of the accumulator object. This is a hash of
    // stream_id + ty
    accumulator_id: Value,
    // The stream ID (the `stream_id` field of some EventStreamCap)
    stream_id: Value,
    // The type of the stream head. Should always be `sui::event::EventStreamHead`
    stream_head_ty: Type,
}

fn emit_impl(
    context: &mut NativeContext,
    ty: Type,
    event_value: Value,
    stream_ref: Option<StreamRef>,
) -> PartialVMResult<NativeResult> {
    let event_emit_cost_params = get_extension!(context, NativesCostTable)?
        .event_emit_cost_params
        .clone();

    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let event_value_size = abstract_size(
        get_extension!(context, ObjectRuntime)?.protocol_config,
        &event_value,
    );

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

    if stream_ref.is_some() {
        native_charge_gas_early_exit!(
            context,
            // this code cannot be reached in protocol versions which don't define
            // event_emit_auth_stream_cost
            event_emit_cost_params.event_emit_auth_stream_cost.unwrap()
        );
    }

    // Get the type tag before getting the mutable reference to avoid borrowing issues
    let stream_head_type_tag = if stream_ref.is_some() {
        Some(context.type_to_type_tag(&stream_ref.as_ref().unwrap().stream_head_ty)?)
    } else {
        None
    };

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    let max_event_emit_size = obj_runtime.protocol_config.max_event_emit_size();
    let ev_size = u64::from(tag_size + event_value_size);
    // Check if the event size is within the limit
    if ev_size > max_event_emit_size {
        return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
            .with_message(format!(
                "Emitting event of size {ev_size} bytes. Limit is {max_event_emit_size} bytes."
            ))
            .with_sub_status(
                VMMemoryLimitExceededSubStatusCode::EVENT_SIZE_LIMIT_EXCEEDED as u64,
            ));
    }

    // Check that the size contribution of the event is within the total size limit
    // This feature is guarded as its only present in some versions
    if let Some(max_event_emit_size_total) = obj_runtime
        .protocol_config
        .max_event_emit_size_total_as_option()
    {
        let total_events_size = obj_runtime.state.total_events_size() + ev_size;
        if total_events_size > max_event_emit_size_total {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!(
                    "Reached total event size of size {total_events_size} bytes. Limit is {max_event_emit_size_total} bytes."
                ))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::TOTAL_EVENT_SIZE_LIMIT_EXCEEDED as u64,
                ));
        }
        obj_runtime.state.incr_total_events_size(ev_size);
    }
    // Emitting an event is cheap since its a vector push
    native_charge_gas_early_exit!(
        context,
        event_emit_cost_params.event_emit_output_cost_per_byte * ev_size.into()
    );

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;

    obj_runtime.emit_event(*tag, event_value)?;

    if let Some(StreamRef {
        accumulator_id,
        stream_id,
        stream_head_ty: _,
    }) = stream_ref
    {
        let stream_id_addr: AccountAddress = stream_id.value_as::<AccountAddress>().unwrap();
        let accumulator_id: ObjectID = accumulator_id.value_as::<AccountAddress>().unwrap().into();
        let events_len = obj_runtime.state.events().len();
        if events_len == 0 {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("No events found after emitting authenticated event".to_string()),
            );
        }
        let event_idx = events_len - 1;
        obj_runtime.emit_accumulator_event(
            accumulator_id,
            MoveAccumulatorAction::Merge,
            stream_id_addr,
            stream_head_type_tag.unwrap(),
            MoveAccumulatorValue::EventRef(event_idx as u64),
        )?;
    }

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

/// Get the all emitted events of type `T`, starting at the specified index
pub fn num_events(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    assert!(args.is_empty());
    let object_runtime_ref: &ObjectRuntime = get_extension!(context)?;
    let num_events = object_runtime_ref.state.events().len();
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![Value::u32(num_events as u32)],
    ))
}

/// Get the all emitted events of type `T`, starting at the specified index
pub fn get_events_by_type(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 1);
    let specified_ty = ty_args.pop().unwrap();
    let specialization: VectorSpecialization = (&specified_ty).try_into()?;
    assert!(args.is_empty());
    let object_runtime_ref: &ObjectRuntime = get_extension!(context)?;
    let specified_type_tag = match context.type_to_type_tag(&specified_ty)? {
        TypeTag::Struct(s) => *s,
        _ => return Ok(NativeResult::ok(legacy_test_cost(), smallvec![])),
    };
    let matched_events = object_runtime_ref
        .state
        .events()
        .iter()
        .filter_map(|(tag, event)| {
            if &specified_type_tag == tag {
                Some(event.copy_value().unwrap())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![move_vm_types::values::Vector::pack(
            specialization,
            matched_events
        )?],
    ))
}
