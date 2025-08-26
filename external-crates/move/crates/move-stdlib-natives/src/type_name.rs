// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    gas_algebra::{InternalGas, InternalGasPerByte, NumBytes},
    language_storage::TypeTag,
};
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

/// Error code when non-module types (not a struct or enum) are passed to `defining_id` or
/// `original_id`.
const E_NON_MODULE_TYPE: u64 = 0;

#[derive(Debug, Clone)]
pub struct GetGasParameters {
    pub base: InternalGas,
    pub per_byte: InternalGasPerByte,
}

#[derive(Debug, Clone)]
pub struct IdGasParameters(Option<IdGasParameters_>);

#[derive(Debug, Clone)]
pub struct IdGasParameters_ {
    pub base: InternalGas,
}

impl IdGasParameters {
    pub fn new(base: Option<impl Into<InternalGas>>) -> Self {
        Self(base.map(|base| IdGasParameters_ { base: base.into() }))
    }
}

fn native_get(
    use_original_id: bool,
    gas_params: &GetGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert!(arguments.is_empty());

    // Charge base fee
    native_charge_gas_early_exit!(context, gas_params.base);

    let type_tag = if use_original_id {
        context.type_to_runtime_type_tag(&ty_args[0])
    } else {
        context.type_to_type_tag(&ty_args[0])
    }?;

    let type_name = type_tag.to_canonical_string(/* with_prefix */ false);

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

fn native_id(
    use_original_id: bool,
    gas_params: &IdGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let gas_params = super::inner_gas_params!(gas_params)?;
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert!(arguments.is_empty());

    // Charge base fee
    native_charge_gas_early_exit!(context, gas_params.base);

    let type_tag = if use_original_id {
        context.type_to_runtime_type_tag(&ty_args[0])
    } else {
        context.type_to_type_tag(&ty_args[0])
    }?;
    Ok(match type_tag {
        TypeTag::Bool
        | TypeTag::U8
        | TypeTag::U64
        | TypeTag::U128
        | TypeTag::Address
        | TypeTag::Signer
        | TypeTag::Vector(_)
        | TypeTag::U16
        | TypeTag::U32
        | TypeTag::U256 => NativeResult::err(context.gas_used(), E_NON_MODULE_TYPE),
        TypeTag::Struct(struct_tag) => {
            let address = struct_tag.address;
            NativeResult::ok(context.gas_used(), smallvec![Value::address(address)])
        }
    })
}

pub fn make_native_get(use_original_id: bool, gas_params: GetGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| {
        native_get(use_original_id, &gas_params, context, ty_args, args)
    })
}

pub fn make_native_id(use_original_id: bool, gas_params: IdGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| {
        native_id(use_original_id, &gas_params, context, ty_args, args)
    })
}

#[derive(Debug, Clone)]
pub struct GasParameters {
    pub get: GetGasParameters,
    pub id: IdGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        (
            "get",
            make_native_get(/* use_original_id */ false, gas_params.get.clone()),
        ),
        (
            "get_with_original_ids",
            make_native_get(/* use_original_id */ true, gas_params.get.clone()),
        ),
        (
            "with_defining_ids",
            make_native_get(/* use_original_id */ false, gas_params.get.clone()),
        ),
        (
            "with_original_ids",
            make_native_get(/* use_original_id */ true, gas_params.get),
        ),
        (
            "defining_id",
            make_native_id(/* use_original_id */ false, gas_params.id.clone()),
        ),
        (
            "original_id",
            make_native_id(/* use_original_id */ true, gas_params.id),
        ),
    ];

    crate::helpers::make_module_natives(natives)
}
