// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_test_cost, types::is_otw_struct};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{gas_algebra::InternalGas, runtime_value::MoveTypeLayout};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub fn destroy(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    args.pop_back();
    Ok(NativeResult::ok(legacy_test_cost(), smallvec![]))
}

pub fn create_one_time_witness(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.is_empty());

    let ty = ty_args.pop().unwrap();
    let type_tag = context.type_to_type_tag(&ty)?;
    let type_layout = context.type_to_type_layout(&ty)?;

    let Some(MoveTypeLayout::Struct(struct_layout)) = type_layout else {
        return Ok(NativeResult::err(InternalGas::new(1), 0));
    };

    let hardened_check = context.runtime_limits_config().hardened_otw_check;
    if is_otw_struct(&struct_layout, &type_tag, hardened_check) {
        Ok(NativeResult::ok(
            legacy_test_cost(),
            smallvec![Value::struct_(move_vm_types::values::Struct::pack(vec![
                Value::bool(true)
            ]))],
        ))
    } else {
        Ok(NativeResult::err(InternalGas::new(1), 1))
    }
}
