// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    object_runtime::{MoveAccumulatorAction, MoveAccumulatorValue, ObjectRuntime},
    NativesCostTable,
};
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::base_types::ObjectID;

pub fn emit_deposit_event(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    emit_event(context, ty_args, args, MoveAccumulatorAction::Merge)
}

pub fn emit_withdraw_event(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    emit_event(context, ty_args, args, MoveAccumulatorAction::Split)
}

fn emit_event(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
    action: MoveAccumulatorAction,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    let event_emit_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .event_emit_cost_params
        .clone();

    // TODO(address-balances): add specific cost for this
    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let amount = args.pop_back().unwrap().value_as::<u64>().unwrap();
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

    let ty_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;

    obj_runtime.emit_accumulator_event(
        accumulator,
        action,
        recipient,
        ty_tag,
        MoveAccumulatorValue::U64(amount),
    )?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

pub fn record_settlement_sui_conservation(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let output_sui = args.pop_back().unwrap().value_as::<u64>().unwrap();
    let input_sui = args.pop_back().unwrap().value_as::<u64>().unwrap();

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;

    obj_runtime.record_settlement_sui_conservation(input_sui, output_sui);

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
