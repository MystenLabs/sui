// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    get_or_fetch_object,
    object_runtime::{
        object_store::ObjectResult, MoveAccumulatorAction, MoveAccumulatorValue, ObjectRuntime,
    },
    NativesCostTable,
};
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Struct, Value},
};
use smallvec::smallvec;
use std::collections::btree_map::Entry;
use std::collections::VecDeque;
use sui_types::{base_types::ObjectID, SUI_ACCUMULATOR_ROOT_OBJECT_ID};

const E_INSUFFICIENT_BALANCE: u64 = 1;
const E_INTERNAL_INVARIANT_VIOLATION: u64 = 2;
const E_ROOT_ACCUMULATOR_VERSION_UNAVAILABLE: u64 = 3;

macro_rules! pop_object_id {
    ($args:ident) => {{
        let o: ObjectID = $args
            .pop_back()
            .unwrap()
            .value_as::<AccountAddress>()
            .unwrap()
            .into();
        o
    }};
}

macro_rules! pop_address {
    ($args:ident) => {
        $args
            .pop_back()
            .unwrap()
            .value_as::<AccountAddress>()
            .unwrap()
    };
}

macro_rules! get_cost_params {
    ($context:ident, $field:ident) => {
        $context
            .extensions_mut()
            .get::<NativesCostTable>()?
            .$field
            .clone()
    };
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

    let event_emit_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .event_emit_cost_params
        .clone();

    // TODO(address-balances): add specific cost for this
    native_charge_gas_early_exit!(context, event_emit_cost_params.event_emit_cost_base);

    let amount = args.pop_back().unwrap().value_as::<u64>().unwrap();
    let recipient = pop_address!(args);
    let accumulator = pop_object_id!(args);

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

macro_rules! load_available_balance_u128 {
    ($context:ident, $accumulator:ident, $accumulator_ty:ident, $gas_cost_per_byte:ident) => {{
        // This code is ugly because we cannot hold a mut ref to ObjectRuntime across any borrow
        // of $context
        let balance = {
            let object_runtime: &mut ObjectRuntime = $context.extensions_mut().get_mut()?;

            let Some(root_accumulator_version) =
                object_runtime.state.root_accumulator_version.as_ref()
            else {
                return Ok(NativeResult::err(
                    $context.gas_used(),
                    E_ROOT_ACCUMULATOR_VERSION_UNAVAILABLE,
                ));
            };

            object_runtime
                .state
                .object_balances
                .get(&$accumulator)
                .copied()
        };

        if let Some(balance) = balance {
            balance
        } else {
            let mut type_args = vec![$accumulator_ty];
            let accumulator_value = get_or_fetch_object!(
                $context,
                type_args,
                SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                $accumulator,
                // XXX - the first transaction each commit will pay slightly more gas for loading
                // the object. is that okay?
                $gas_cost_per_byte
            );

            let ObjectResult::Loaded(gv) = accumulator_value else {
                // This should never happen, but the chain can actually continue
                // running with this error. The user will be unable to withdraw,
                // but they won't be able to withdraw if we panic either.
                if cfg!(debug_assertions) {
                    panic!("accumulator value type mismatch");
                }
                return Ok(NativeResult::err(
                    $context.gas_used(),
                    E_INTERNAL_INVARIANT_VIOLATION,
                ));
            };

            let value = gv.borrow_global()?;
            let s: Struct = value.value_as()?;
            let mut fields = s.unpack()?.collect::<Vec<_>>();
            if fields.len() != 1 {
                // TODO: return an error?
                panic!("U128 should have 1 field, got {}", fields.len());
            }
            let field = fields.pop().unwrap();
            let balance = field.value_as::<u128>()?;

            let object_runtime: &mut ObjectRuntime = $context.extensions_mut().get_mut()?;
            object_runtime
                .state
                .object_balances
                .insert($accumulator, balance);
            balance
        }
    }};
}

fn get_available_object_balance_u128(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let available_balance = {
        let accumulator = pop_object_id!(args);
        let accumulator_ty = ty_args.pop().unwrap();

        let dynamic_field_borrow_child_object_type_cost_per_byte =
            get_cost_params!(context, dynamic_field_borrow_child_object_cost_params)
                .dynamic_field_borrow_child_object_type_cost_per_byte;

        let available_balance = load_available_balance_u128!(
            context,
            accumulator,
            accumulator_ty,
            dynamic_field_borrow_child_object_type_cost_per_byte
        );
        available_balance
    };

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u128(available_balance)],
    ))
}

fn emit_withdraw_from_object_balance_event(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    let ty = ty_args.pop().unwrap();

    let amount = args.pop_back().unwrap().value_as::<u64>().unwrap();
    let object = pop_address!(args);
    let accumulator = pop_object_id!(args);
    let accumulator_ty = ty_args.pop().unwrap();

    let dynamic_field_borrow_child_object_type_cost_per_byte =
        get_cost_params!(context, dynamic_field_borrow_child_object_cost_params)
            .dynamic_field_borrow_child_object_type_cost_per_byte;

    let available_balance = load_available_balance_u128!(
        context,
        accumulator,
        accumulator_ty,
        dynamic_field_borrow_child_object_type_cost_per_byte
    );

    if amount as u128 > available_balance {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_INSUFFICIENT_BALANCE,
        ));
    }

    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;

    let new_balance = available_balance - amount as u128;
    object_runtime
        .state
        .object_balances
        .insert(accumulator, new_balance);

    object_runtime.emit_accumulator_event(
        accumulator,
        MoveAccumulatorAction::Split,
        object,
        ty,
        MoveAccumulatorValue::U64(amount),
    )?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
