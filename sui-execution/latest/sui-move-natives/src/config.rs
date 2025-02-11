// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, language_storage::StructTag,
    runtime_value as R, vm_status::StatusCode,
};
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Struct, Value, Vector},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{base_types::MoveObjectType, TypeTag};
use tracing::{error, instrument};

const E_BCS_SERIALIZATION_FAILURE: u64 = 2;

#[derive(Clone)]
pub struct ConfigReadSettingImplCostParams {
    pub config_read_setting_impl_cost_base: Option<InternalGas>,
    pub config_read_setting_impl_cost_per_byte: Option<InternalGas>,
}

#[instrument(level = "trace", skip_all)]
pub fn read_setting_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 4);
    assert_eq!(args.len(), 3);

    let ConfigReadSettingImplCostParams {
        config_read_setting_impl_cost_base,
        config_read_setting_impl_cost_per_byte,
    } = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .config_read_setting_impl_cost_params
        .clone();

    let config_read_setting_impl_cost_base =
        config_read_setting_impl_cost_base.ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("gas cost is not set".to_string())
        })?;
    let config_read_setting_impl_cost_per_byte = config_read_setting_impl_cost_per_byte
        .ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("gas cost is not set".to_string())
        })?;
    // Charge base fee
    native_charge_gas_early_exit!(context, config_read_setting_impl_cost_base);

    let value_ty = ty_args.pop().unwrap();
    let setting_data_value_ty = ty_args.pop().unwrap();
    let setting_value_ty = ty_args.pop().unwrap();
    let field_setting_ty = ty_args.pop().unwrap();

    let current_epoch = pop_arg!(args, u64);
    let name_df_addr = pop_arg!(args, AccountAddress);
    let config_addr = pop_arg!(args, AccountAddress);

    let field_setting_tag: StructTag = match context.type_to_type_tag(&field_setting_ty)? {
        TypeTag::Struct(s) => *s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let Some(field_setting_layout) = context.type_to_type_layout(&field_setting_ty)? else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_BCS_SERIALIZATION_FAILURE,
        ));
    };
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();

    let read_value_opt = consistent_value_before_current_epoch(
        object_runtime,
        &field_setting_ty,
        field_setting_tag,
        &field_setting_layout,
        &setting_value_ty,
        &setting_data_value_ty,
        &value_ty,
        config_addr,
        name_df_addr,
        current_epoch,
    )?;

    native_charge_gas_early_exit!(
        context,
        config_read_setting_impl_cost_per_byte * u64::from(read_value_opt.legacy_size()).into()
    );

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![read_value_opt],
    ))
}

fn consistent_value_before_current_epoch(
    object_runtime: &mut ObjectRuntime,
    field_setting_ty: &Type,
    field_setting_tag: StructTag,
    field_setting_layout: &R::MoveTypeLayout,
    _setting_value_ty: &Type,
    setting_data_value_ty: &Type,
    value_ty: &Type,
    config_addr: AccountAddress,
    name_df_addr: AccountAddress,
    current_epoch: u64,
) -> PartialVMResult<Value> {
    let field_setting_obj_ty = MoveObjectType::from(field_setting_tag);
    let Some(field) = object_runtime.config_setting_unsequenced_read(
        config_addr.into(),
        name_df_addr.into(),
        field_setting_ty,
        field_setting_layout,
        &field_setting_obj_ty,
    ) else {
        return option_none(value_ty);
    };

    let [_id, _name, setting]: [Value; 3] = unpack_struct(field)?;
    let [data_opt]: [Value; 1] = unpack_struct(setting)?;
    let data = match unpack_option(data_opt, setting_data_value_ty)? {
        None => {
            error!(
                "
                SettingData is none.
                config_addr: {config_addr},
                name_df_addr: {name_df_addr},
                field_setting_obj_ty: {field_setting_obj_ty:?}",
            );
            return option_none(value_ty);
        }
        Some(data) => data,
    };
    let [newer_value_epoch, newer_value, older_value_opt]: [Value; 3] = unpack_struct(data)?;
    let newer_value_epoch: u64 = newer_value_epoch.value_as()?;
    debug_assert!(
        unpack_option(newer_value.copy_value()?, value_ty)?.is_some()
            || unpack_option(older_value_opt.copy_value()?, value_ty)?.is_some()
    );
    Ok(if current_epoch > newer_value_epoch {
        newer_value
    } else {
        older_value_opt
    })
}

fn unpack_struct<const N: usize>(s: Value) -> PartialVMResult<[Value; N]> {
    let s: Struct = s.value_as()?;
    s.unpack()?.collect::<Vec<_>>().try_into().map_err(|e| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
            .with_message(format!("struct expected to have {N} fields: {e:?}"))
    })
}

fn unpack_option(option: Value, type_param: &Type) -> PartialVMResult<Option<Value>> {
    let [vec_value]: [Value; 1] = unpack_struct(option)?;
    let vec: Vector = vec_value.value_as()?;
    Ok(if vec.elem_len() == 0 {
        None
    } else {
        let [elem]: [Value; 1] = vec.unpack(type_param, 1)?.try_into().map_err(|e| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!("vector expected to have one element: {e:?}"))
        })?;
        Some(elem)
    })
}

fn option_none(type_param: &Type) -> PartialVMResult<Value> {
    Ok(Value::struct_(Struct::pack(vec![Vector::empty(
        type_param,
    )?])))
}
