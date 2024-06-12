// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    object_runtime::{object_store::ObjectResult, ObjectRuntime},
    NativesCostTable,
};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, vm_status::StatusCode,
};
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Reference, Struct, StructRef, Value, Vector, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use tracing::instrument;

const E_READ_SETTING_FAILED: u64 = 2;

#[derive(Clone)]
pub struct ConfigReadSettingImplCostParams {
    pub config_read_setting_impl_cost_base: InternalGas,
    pub config_read_setting_impl_cost_per_byte: InternalGas,
}

#[instrument(level = "trace", skip_all, err)]
pub fn read_setting_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 4);
    assert_eq!(args.len(), 3);

    let config_read_setting_impl_cost_params: ConfigReadSettingImplCostParams = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .config_read_setting_impl_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        config_read_setting_impl_cost_params.config_read_setting_impl_cost_base
    );

    let value_ty = ty_args.pop().unwrap();
    let setting_data_value_ty = ty_args.pop().unwrap();
    let setting_value_ty = ty_args.pop().unwrap();
    let key_type = ty_args.pop().unwrap();

    let current_epoch = pop_arg!(args, u64);
    let name_df_addr = pop_arg!(args, AccountAddress);
    let config_addr = pop_arg!(args, AccountAddress);

    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();

    let read_value_opt = consistent_value_before_current_epoch(
        object_runtime,
        &setting_value_ty,
        &setting_data_value_ty,
        &value_ty,
        config_addr,
        name_df_addr,
        current_epoch,
    )?;

    native_charge_gas_early_exit!(
        context,
        config_read_setting_impl_cost_params.config_read_setting_impl_cost_per_byte
            * u64::from(read_value_opt.legacy_size()).into()
    );

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![read_value_opt],
    ))
}

fn consistent_value_before_current_epoch(
    object_runtime: &mut ObjectRuntime,
    setting_value_ty: &Type,
    setting_data_value_ty: &Type,
    value_ty: &Type,
    config_addr: AccountAddress,
    name_df_addr: AccountAddress,
    current_epoch: u64,
) -> PartialVMResult<Value> {
    let global_value = match object_runtime.config_setting_unsequenced_read(
        config_addr,
        name_df_addr,
        current_epoch,
    ) {
        ObjectResult::MismatchedType => return option_none(&value_ty),
        ObjectResult::Loaded(gv) => gv,
    };
    if !global_value.exists()? {
        return option_none(&value_ty);
    }
    let setting_ref: Value = global_value.borrow_global().map_err(|err| {
        assert!(err.major_status() != StatusCode::MISSING_DATA);
        err
    })?;
    let setting_ref: StructRef = setting_ref.value_as()?;
    let data_opt_ref: StructRef = setting_ref.borrow_field(1)?.value_as()?;
    let data_ref = match borrow_option_value(data_opt_ref, &setting_data_value_ty)? {
        None => {
            // invariant violation?
            return option_none(&value_ty);
        }
        Some(data_ref) => data_ref,
    };
    let data_ref: StructRef = data_ref.value_as()?;
    let newer_value_epoch: u64 = data_ref.borrow_field(0)?.value_as()?;
    if current_epoch > newer_value_epoch {
        let newer_value_ref: Reference = data_ref.borrow_field(1)?.value_as()?;
        let newer_value = newer_value_ref.read_ref()?;
        option_some(&value_ty, newer_value)
    } else {
        let older_value_opt_ref: Reference = data_ref.borrow_field(2)?.value_as()?;
        older_value_opt_ref.read_ref()
    }
}

fn borrow_option_value(option_ref: StructRef, type_param: &Type) -> PartialVMResult<Option<Value>> {
    let vec_ref: VectorRef = option_ref.borrow_field(0)?.value_as()?;
    if vec_ref.len(&type_param)?.value_as::<u64>()? == 0 {
        return Ok(None);
    }
    Ok(Some(vec_ref.borrow_elem(0, &type_param)?))
}

fn option_none(type_param: &Type) -> PartialVMResult<Value> {
    Ok(Value::struct_(Struct::pack(vec![Vector::empty(
        type_param,
    )?])))
}

fn option_some(type_param: &Type, value: Value) -> PartialVMResult<Value> {
    Ok(Value::struct_(Struct::pack(vec![Vector::pack(
        type_param,
        vec![value],
    )?])))
}
