// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{account_address::AccountAddress, u256::U256, vm_status::StatusCode};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Struct, Value},
};
use smallvec::smallvec;
use sui_types::base_types::ObjectID;

use crate::{
    object_runtime::{MoveAccumulatorAction, MoveAccumulatorValue, ObjectRuntime},
    NativesCostTable,
};

const E_OVERFLOW: u64 = 0;

pub fn add_to_accumulator_address(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    // TODO(address-balances): add specific cost for this
    let event_emit_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .event_emit_cost_params
        .clone();
    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let ty_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    let Some(value) = args.pop_back().unwrap().value_as::<Struct>().ok() else {
        // TODO in the future this is guaranteed/checked via a custom verifier rule
        debug_assert!(false);
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                "Balance should be guaranteed under current implementation".to_owned(),
            ),
        );
    };
    let recipient = args
        .pop_back()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap();
    let accumulator: ObjectID = args
        .pop_back()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();

    // TODO this will need to look at the layout of T when this is not guaranteed to be a Balance
    let Some([amount]): Option<[Value; 1]> = value
        .unpack()
        .ok()
        .and_then(|vs| vs.collect::<Vec<_>>().try_into().ok())
    else {
        debug_assert!(false);
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                "Balance should be guaranteed under current implementation".to_owned(),
            ),
        );
    };
    let Some(amount) = amount.value_as::<u64>().ok() else {
        debug_assert!(false);
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                "Balance should be guaranteed under current implementation".to_owned(),
            ),
        );
    };

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;
    obj_runtime.emit_accumulator_event(
        accumulator,
        MoveAccumulatorAction::Merge,
        recipient,
        ty_tag,
        MoveAccumulatorValue::U64(amount),
    )?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

pub fn withdraw_from_accumulator_address(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    // TODO(address-balances): add specific cost for this
    // TODO(address-balances): determine storage cost for "Merge"
    let event_emit_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .event_emit_cost_params
        .clone();
    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let ty_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    let value = args.pop_back().unwrap().value_as::<U256>().unwrap();
    let recipient = args
        .pop_back()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap();
    let accumulator: ObjectID = args
        .pop_back()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();

    // TODO this will need to look at the layout of T when this is not guaranteed to be a Balance
    let Ok(amount): Result<u64, _> = value.try_into() else {
        return Ok(NativeResult::err(context.gas_used(), E_OVERFLOW));
    };

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;
    obj_runtime.emit_accumulator_event(
        accumulator,
        MoveAccumulatorAction::Split,
        recipient,
        ty_tag,
        MoveAccumulatorValue::U64(amount),
    )?;
    // TODO this will need to look at the layout of T when this is not guaranteed to be a Balance
    let withdrawn = Value::struct_(Struct::pack(vec![Value::u64(amount)]));
    Ok(NativeResult::ok(context.gas_used(), smallvec![withdrawn]))
}
