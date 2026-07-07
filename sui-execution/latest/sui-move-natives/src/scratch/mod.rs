// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod runtime;

pub use runtime::ScratchRuntime;

use crate::{NativesCostTable, get_extension, get_extension_mut};
use move_binary_format::errors::PartialVMResult;
use move_binary_format::{safe_assert, safe_assert_eq, safe_unwrap};
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::natives::functions::NativeContext;
use move_vm_runtime::{
    execution::{Type, values::Value},
    natives::functions::NativeResult,
    pop_arg,
    shared::views::{SizeConfig, ValueView},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use tracing::instrument;

// These must match the error constants declared in `sui::scratch`.
const E_ENTRY_ALREADY_EXISTS: u64 = 0;
const E_ENTRY_DOES_NOT_EXIST: u64 = 1;
const E_ENTRY_TYPE_MISMATCH: u64 = 2;

/// Abstract size of a scratch value, used to cost the copy performed by `read`.
fn value_size(value: &Value) -> PartialVMResult<u64> {
    Ok(value
        .abstract_memory_size(&SizeConfig {
            include_vector_size: true,
            traverse_references: false,
        })?
        .into())
}

#[derive(Clone)]
pub struct ScratchAddCostParams {
    pub scratch_add_cost_base: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun add_impl
 * throws `E_ENTRY_ALREADY_EXISTS` if there is already an entry for `key`, regardless of the type
 * of `V`
 * Implementation of the Move native function `add_impl<V: drop>(key: address, value: V)`
 *   gas cost: scratch_add_cost_base                    | fixed cost, the value is moved into the
 *                                                        store so its size is irrelevant
 **************************************************************************************************/
#[instrument(level = "trace", skip_all)]
pub fn add_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 1);
    safe_assert_eq!(args.len(), 2);

    let scratch_add_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .scratch_add_cost_params
            .scratch_add_cost_base
    );
    native_charge_gas_early_exit!(context, scratch_add_cost_base);

    let value = safe_unwrap!(args.pop_back());
    let key = pop_arg!(args, AccountAddress);
    safe_assert!(args.is_empty());
    let ty = safe_unwrap!(ty_args.pop());

    let scratch_runtime: &mut ScratchRuntime = get_extension_mut!(context)?;
    if scratch_runtime.add(key, ty, value).is_err() {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_ENTRY_ALREADY_EXISTS,
        ));
    }

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

#[derive(Clone)]
pub struct ScratchReadCostParams {
    pub scratch_read_cost_base: Option<InternalGas>,
    pub scratch_read_value_cost: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun read_impl
 * throws `E_ENTRY_DOES_NOT_EXIST` if there is no entry for `key`
 * or throws `E_ENTRY_TYPE_MISMATCH` if the entry's value is not of type `V`
 * Implementation of the Move native function `read_impl<V: copy + drop>(key: address): V`
 *   gas cost: scratch_read_cost_base                                     | fixed cost
 *              + scratch_read_value_cost * size_of(value)       | covers copying the value out
 **************************************************************************************************/
#[instrument(level = "trace", skip_all)]
pub fn read_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 1);
    safe_assert_eq!(args.len(), 1);

    let ScratchReadCostParams {
        scratch_read_cost_base,
        scratch_read_value_cost,
    } = get_extension!(context, NativesCostTable)?
        .scratch_read_cost_params
        .clone();
    let scratch_read_cost_base = safe_unwrap!(scratch_read_cost_base);
    let scratch_read_value_cost = safe_unwrap!(scratch_read_value_cost);
    native_charge_gas_early_exit!(context, scratch_read_cost_base);

    let key = pop_arg!(args, AccountAddress);
    safe_assert!(args.is_empty());
    let ty = safe_unwrap!(ty_args.pop());

    let scratch_runtime: &ScratchRuntime = get_extension!(context)?;
    let entry = match scratch_runtime.get(&key) {
        None => {
            return Ok(NativeResult::err(
                context.gas_used(),
                E_ENTRY_DOES_NOT_EXIST,
            ));
        }
        Some(entry) if entry.ty != ty => {
            return Ok(NativeResult::err(context.gas_used(), E_ENTRY_TYPE_MISMATCH));
        }
        Some(entry) => entry,
    };

    native_charge_gas_early_exit!(
        context,
        scratch_read_value_cost * value_size(&entry.value)?.into()
    );
    let value = entry.value.copy_value();

    Ok(NativeResult::ok(context.gas_used(), smallvec![value]))
}

#[derive(Clone)]
pub struct ScratchRemoveCostParams {
    pub scratch_remove_cost_base: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun remove_impl
 * throws `E_ENTRY_DOES_NOT_EXIST` if there is no entry for `key`
 * or throws `E_ENTRY_TYPE_MISMATCH` if the entry's value is not of type `V`
 * Implementation of the Move native function `remove_impl<V: drop>(key: address): V`
 *   gas cost: scratch_remove_cost_base                 | fixed cost, the value is moved out of the
 *                                                        store so its size is irrelevant
 **************************************************************************************************/
#[instrument(level = "trace", skip_all)]
pub fn remove_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 1);
    safe_assert_eq!(args.len(), 1);

    let scratch_remove_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .scratch_remove_cost_params
            .scratch_remove_cost_base
    );
    native_charge_gas_early_exit!(context, scratch_remove_cost_base);

    let key = pop_arg!(args, AccountAddress);
    safe_assert!(args.is_empty());
    let ty = safe_unwrap!(ty_args.pop());

    let scratch_runtime: &mut ScratchRuntime = get_extension_mut!(context)?;
    let Some(entry) = scratch_runtime.remove(&key) else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_ENTRY_DOES_NOT_EXIST,
        ));
    };

    if entry.ty != ty {
        return Ok(NativeResult::err(context.gas_used(), E_ENTRY_TYPE_MISMATCH));
    }

    Ok(NativeResult::ok(context.gas_used(), smallvec![entry.value]))
}

#[derive(Clone)]
pub struct ScratchExistsCostParams {
    pub scratch_exists_cost_base: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun exists_impl
 * Implementation of the Move native function `exists_impl(key: address): bool`
 *   gas cost: scratch_exists_cost_base                 | fixed cost, this is a lookup
 **************************************************************************************************/
#[instrument(level = "trace", skip_all)]
pub fn exists_impl(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert!(ty_args.is_empty());
    safe_assert_eq!(args.len(), 1);

    let scratch_exists_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .scratch_exists_cost_params
            .scratch_exists_cost_base
    );
    native_charge_gas_early_exit!(context, scratch_exists_cost_base);

    let key = pop_arg!(args, AccountAddress);
    let scratch_runtime: &ScratchRuntime = get_extension!(context)?;
    let exists = scratch_runtime.contains(&key);
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(exists)],
    ))
}

#[derive(Clone)]
pub struct ScratchExistsWithTypeCostParams {
    pub scratch_exists_with_type_cost_base: Option<InternalGas>,
    pub scratch_exists_with_type_type_cost: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun exists_with_type_impl
 * Implementation of the Move native function `exists_with_type_impl<V: drop>(key: address): bool`
 *   gas cost: scratch_exists_with_type_cost_base                        | fixed cost
 *              + scratch_exists_with_type_type_cost * size_of(V)        | covers operating on type `V`
 **************************************************************************************************/
#[instrument(level = "trace", skip_all)]
pub fn exists_with_type_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 1);
    safe_assert_eq!(args.len(), 1);

    let ScratchExistsWithTypeCostParams {
        scratch_exists_with_type_cost_base,
        scratch_exists_with_type_type_cost,
    } = get_extension!(context, NativesCostTable)?
        .scratch_exists_with_type_cost_params
        .clone();
    let scratch_exists_with_type_cost_base = safe_unwrap!(scratch_exists_with_type_cost_base);
    let scratch_exists_with_type_type_cost = safe_unwrap!(scratch_exists_with_type_type_cost);
    native_charge_gas_early_exit!(context, scratch_exists_with_type_cost_base);

    let key = pop_arg!(args, AccountAddress);
    safe_assert!(args.is_empty());
    let ty = safe_unwrap!(ty_args.pop());

    native_charge_gas_early_exit!(
        context,
        scratch_exists_with_type_type_cost * u64::from(ty.size()?).into()
    );

    let scratch_runtime: &ScratchRuntime = get_extension!(context)?;
    let exists = scratch_runtime
        .get(&key)
        .is_some_and(|entry| entry.ty == ty);
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(exists)],
    ))
}
