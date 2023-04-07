// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::{InternalGas, InternalGasPerByte, NumBytes};
use move_vm_runtime::{
    native_charge_gas_early_exit,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Struct, Value},
};

use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug, Clone)]
pub struct GetGasParameters {
    pub base: InternalGas,
    pub per_byte: InternalGasPerByte,
}

fn native_get(
    gas_params: &GetGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert!(arguments.is_empty());

    // Charge base fee
    native_charge_gas_early_exit!(context, gas_params.base);

    let type_tag = context.type_to_type_tag(&ty_args[0])?;
    let type_name = type_tag.to_canonical_string();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        gas_params.per_byte * NumBytes::new(type_name.len() as u64)
    );

    // make a std::string::String
    let string_val = Value::struct_(Struct::pack(vec![Value::vector_u8(
        type_name.as_bytes().to_vec(),
    )]));
    // make a std::type_name::TypeName
    let type_name_val = Value::struct_(Struct::pack(vec![string_val]));

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![type_name_val],
    ))
}

pub fn make_native_get(gas_params: GetGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| native_get(&gas_params, context, ty_args, args))
}

#[derive(Debug, Clone)]
pub struct GasParameters {
    pub get: GetGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [("get", make_native_get(gas_params.get))];

    crate::natives::helpers::make_module_natives(natives)
}
