// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NativesCostTable, get_extension, get_extension_mut, object_runtime::ObjectRuntime};
use move_binary_format::errors::PartialVMResult;
use move_binary_format::safe_unwrap;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{
    execution::{
        Type,
        values::{StructRef, Value},
    },
    natives::functions::NativeResult,
    pop_arg,
};
use move_vm_runtime::{native_charge_gas_early_exit, natives::functions::NativeContext};
use smallvec::smallvec;
use std::collections::VecDeque;

#[derive(Clone)]
pub struct BorrowUidCostParams {
    pub object_borrow_uid_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun borrow_uid
 * Implementation of the Move native function `borrow_uid<T: key>(obj: &T): &UID`
 *   gas cost: object_borrow_uid_cost_base                | this is hard to calculate as it's very sensitive to `borrow_field` impl. Making it flat
 **************************************************************************************************/
pub fn borrow_uid(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let borrow_uid_cost_params = get_extension!(context, NativesCostTable)?
        .borrow_uid_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(context, borrow_uid_cost_params.object_borrow_uid_cost_base);

    let obj = pop_arg!(args, StructRef);
    let id_field = obj.borrow_field(0)?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![id_field]))
}

#[derive(Clone)]
pub struct DeleteImplCostParams {
    pub object_delete_impl_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun delete_impl
 * Implementation of the Move native function `delete_impl(id: address)`
 *   gas cost: cost_base                | this is a simple ID deletion
 **************************************************************************************************/
pub fn delete_impl(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let delete_impl_cost_params = get_extension!(context, NativesCostTable)?
        .delete_impl_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        delete_impl_cost_params.object_delete_impl_cost_base
    );

    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    obj_runtime.delete_id(uid_bytes.into())?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

#[derive(Clone)]
pub struct RecordNewIdCostParams {
    pub object_record_new_uid_cost_base: InternalGas,
    pub object_record_new_uid_from_hash_cost_base: Option<InternalGas>,
}
/***************************************************************************************************
 * native fun record_new_uid
 * Implementation of the Move native function `record_new_uid(id: address)`
 *   gas cost: object_record_new_uid_cost_base                | this is a simple ID addition
 **************************************************************************************************/
pub fn record_new_uid(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let record_new_id_cost_params = get_extension!(context, NativesCostTable)?
        .record_new_id_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        record_new_id_cost_params.object_record_new_uid_cost_base
    );

    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    obj_runtime.new_id(uid_bytes.into())?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

/***************************************************************************************************
 * native fun record_new_uid_from_hash
 * Implementation of the Move native function
 *   `record_new_uid_from_hash(parent: address, bytes: address)`
 * Marks `bytes` as a newly created id (as `record_new_uid` does) and, when `parent` has a tracked
 * root version, records the same root version for `bytes`.
 *   gas cost: object_record_new_uid_from_hash_cost_base       | this is a simple ID addition
 **************************************************************************************************/
pub fn record_new_uid_from_hash(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let object_record_new_uid_from_hash_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .record_new_id_cost_params
            .object_record_new_uid_from_hash_cost_base
    );

    native_charge_gas_early_exit!(context, object_record_new_uid_from_hash_cost_base);

    let uid_bytes = pop_arg!(args, AccountAddress);
    let parent = pop_arg!(args, AccountAddress);

    let obj_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    obj_runtime.new_id_from_hash(parent.into(), uid_bytes.into())?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
