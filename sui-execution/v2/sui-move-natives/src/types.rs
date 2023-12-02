// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    gas_algebra::InternalGas,
    language_storage::TypeTag,
    runtime_value::{MoveStructLayout, MoveTypeLayout},
};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

use crate::NativesCostTable;

pub(crate) fn is_otw_struct(struct_layout: &MoveStructLayout, type_tag: &TypeTag) -> bool {
    let has_one_bool_field = matches!(struct_layout.0.as_slice(), [MoveTypeLayout::Bool]);

    // If a struct type has the same name as the module that defines it but capitalized, and it has
    // a single field of type bool, it means that it's a one-time witness type. The remaining
    // properties of a one-time witness type are checked in the one_time_witness_verifier pass in
    // the Sui bytecode verifier (a type with this name and with a single bool field that does not
    // have all the remaining properties of a one-time witness type will cause a verifier error).
    matches!(
        type_tag,
        TypeTag::Struct(struct_tag) if has_one_bool_field && struct_tag.name.to_string() == struct_tag.module.to_string().to_ascii_uppercase())
}

#[derive(Clone)]
pub struct TypesIsOneTimeWitnessCostParams {
    pub types_is_one_time_witness_cost_base: InternalGas,
    pub types_is_one_time_witness_type_tag_cost_per_byte: InternalGas,
    pub types_is_one_time_witness_type_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun is_one_time_witness
 * Implementation of the Move native function `is_one_time_witness<T: drop>(_: &T): bool`
 *   gas cost: types_is_one_time_witness_cost_base                        | base cost as this can be expensive oper
 *              + types_is_one_time_witness_type_tag_cost_per_byte * type_tag.size()        | cost per byte of converting type to type tag
 *              + types_is_one_time_witness_type_cost_per_byte * ty.size()                  | cost per byte of converting type to type layout
 **************************************************************************************************/
pub fn is_one_time_witness(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let type_is_one_time_witness_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .type_is_one_time_witness_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        type_is_one_time_witness_cost_params.types_is_one_time_witness_cost_base
    );

    // unwrap safe because the interface of native function guarantees it.
    let ty = ty_args.pop().unwrap();

    native_charge_gas_early_exit!(
        context,
        type_is_one_time_witness_cost_params.types_is_one_time_witness_type_cost_per_byte
            * u64::from(ty.size()).into()
    );

    let type_tag = context.type_to_type_tag(&ty)?;
    native_charge_gas_early_exit!(
        context,
        type_is_one_time_witness_cost_params.types_is_one_time_witness_type_tag_cost_per_byte
            * u64::from(type_tag.abstract_size_for_gas_metering()).into()
    );

    let type_layout = context.type_to_type_layout(&ty)?;

    let cost = context.gas_used();
    let Some(MoveTypeLayout::Struct(struct_layout)) = type_layout else {
        return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)]));
    };

    let is_otw = is_otw_struct(&struct_layout, &type_tag);

    Ok(NativeResult::ok(cost, smallvec![Value::bool(is_otw)]))
}
