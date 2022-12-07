// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_emit_cost, legacy_length_cost};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    language_storage::TypeTag,
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

const E_BCS_SERIALIZATION_FAILURE: u64 = 1;

pub fn is_one_time_witness(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    // unwrap safe because the interface of native function guarantees it.
    let ty = ty_args.pop().unwrap();
    let type_tag = context.type_to_type_tag(&ty)?;
    let type_layout = context.type_to_type_layout(&ty)?;

    // TODO: what should the cost of this be?
    let cost = legacy_length_cost();
    let struct_layout = match type_layout {
        Some(MoveTypeLayout::Struct(l)) => l,
        _ => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let has_one_bool_field = match struct_layout {
        MoveStructLayout::Runtime(vec) => matches!(vec.as_slice(), [MoveTypeLayout::Bool]),
        MoveStructLayout::WithFields(vec) => matches!(
            vec.as_slice(),
            [MoveFieldLayout {
                name: _,
                layout: MoveTypeLayout::Bool
            }]
        ),
        MoveStructLayout::WithTypes { type_: _, fields } => matches!(
            fields.as_slice(),
            [MoveFieldLayout {
                name: _,
                layout: MoveTypeLayout::Bool
            }]
        ),
    };

    // If a struct type has the same name as the module that defines it but capitalized, and it has
    // a single field of type bool, it means that it's a one-time witness type. The remaining
    // properties of a one-time witness type are checked in the one_time_witness_verifier pass in
    // the Sui bytecode verifier (a type with this name and with a single bool field that does not
    // have all the remaining properties of a one-time witness type will cause a verifier error).
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(
            matches!(type_tag, TypeTag::Struct(struct_tag) if has_one_bool_field && struct_tag.name.to_string() == struct_tag.module.to_string().to_ascii_uppercase())
        )],
    ))
}

// native fun type_tag_bytes<T>(): address;
pub fn type_tag_bytes(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 0);
    let ty = ty_args.pop().unwrap();
    let tag = context.type_to_type_tag(&ty)?;
    // build bytes
    let tag_bytes = match bcs::to_bytes(&tag) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(NativeResult::err(
                legacy_emit_cost(),
                E_BCS_SERIALIZATION_FAILURE,
            ));
        }
    };
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::vector_u8(tag_bytes)],
    ))
}
