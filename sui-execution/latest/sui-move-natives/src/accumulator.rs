// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    NativesCostTable, get_extension, get_extension_mut,
    object_runtime::{MoveAccumulatorAction, MoveAccumulatorValue, ObjectRuntime},
};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::base_types::ObjectID;

#[derive(Clone)]
pub struct AccumulatorEmitCostParams {
    pub accumulator_emit_cost_base: InternalGas,
}

#[derive(Clone)]
pub struct AccumulatorPendingBalanceCostParams {
    pub accumulator_pending_balance_cost_base: InternalGas,
}

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

    let accumulator_emit_cost_params = get_extension!(context, NativesCostTable)?
        .accumulator_emit_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        accumulator_emit_cost_params.accumulator_emit_cost_base
    );

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

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;

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

pub fn pending_deposits(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    pending_balance_changes(context, ty_args, args, MoveAccumulatorAction::Merge)
}

pub fn pending_withdrawals(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    pending_balance_changes(context, ty_args, args, MoveAccumulatorAction::Split)
}

fn pending_balance_changes(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
    action: MoveAccumulatorAction,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let accumulator_pending_balance_cost_params = get_extension!(context, NativesCostTable)?
        .accumulator_pending_balance_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        accumulator_pending_balance_cost_params.accumulator_pending_balance_cost_base
    );

    let address = args
        .pop_back()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap();

    let ty_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    let obj_runtime: &ObjectRuntime = get_extension!(context)?;

    let key = (address, ty_tag);
    let totals = match action {
        MoveAccumulatorAction::Merge => &obj_runtime.state.accumulator_merge_totals,
        MoveAccumulatorAction::Split => &obj_runtime.state.accumulator_split_totals,
    };
    let total = totals.get(&key).copied().unwrap_or(0);

    // Clamp to u64::MAX
    let result = std::cmp::min(total, u64::MAX as u128) as u64;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(result)],
    ))
}
