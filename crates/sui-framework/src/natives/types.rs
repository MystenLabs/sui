// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_length_cost;
use move_binary_format::errors::PartialVMResult;
use move_core_types::language_storage::TypeTag;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub fn is_one_time_witness(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    // unwrap safe because the interface of native function guarantees it.
    let type_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    // TODO: what should the cost of this be?
    let cost = legacy_length_cost();

    // If a struct type has the same name as the module that defines it but capitalized, it means
    // that it's a characteristic type (which is one way of implementing a one-time witness
    // type). This is checked in the char_type validator pass (a type with this type of name that
    // does not have all properties required of a characteristic type will cause a validator error).
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(
            matches!(type_tag, TypeTag::Struct(struct_tag) if struct_tag.name.to_string() == struct_tag.module.to_string().to_ascii_uppercase())
        )],
    ))
}
