// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{StructRef, Value},
};
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

    let borrow_uid_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
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

    let delete_impl_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .delete_impl_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        delete_impl_cost_params.object_delete_impl_cost_base
    );

    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.delete_id(uid_bytes.into())?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

#[derive(Clone)]
pub struct RecordNewIdCostParams {
    pub object_record_new_uid_cost_base: InternalGas,
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

    let record_new_id_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .record_new_id_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        record_new_id_cost_params.object_record_new_uid_cost_base
    );

    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.new_id(uid_bytes.into())?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
