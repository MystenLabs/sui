// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::values::{Value, values_impl::Reference},
    jit::execution::ast::Type,
    native_charge_gas_early_exit,
    natives::{
        functions::{NativeContext, NativeFunction, NativeResult},
        make_module_natives,
    },
    pop_arg,
    shared::views::{SizeConfig, ValueView},
};
use move_binary_format::{errors::PartialVMResult, safe_assert, safe_assert_eq, safe_unwrap};
use move_core_types::{
    gas_algebra::InternalGas, vm_status::sub_status::NFE_BCS_SERIALIZATION_FAILURE,
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};
/***************************************************************************************************
 * native fun to_bytes
 *
 *   gas cost: layout_cost(val_type) +
 *             size_of(input) * input_unit_cost +
 *             max(size_of(output), legacy_min_output_size) * output_unit_cost
 *
 *             Layout or serialization failures additionally attempt to charge failure_cost.
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct ToBytesGasParameters {
    pub per_input_abstract_unit: Option<InternalGas>,
    pub per_byte_serialized: InternalGas,
    pub legacy_min_output_size: u64,
    pub failure: InternalGas,
}

/// Rust implementation of Move's `native public fun to_bytes<T>(&T): vector<u8>`
#[inline]
fn native_to_bytes(
    gas_params: &ToBytesGasParameters,
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 1);
    safe_assert_eq!(args.len(), 1);

    // pop type and value
    let ref_to_val = pop_arg!(args, Reference);
    let arg_type = safe_unwrap!(ty_args.pop());

    // get type layout
    let layout = match context.type_to_type_layout(&arg_type)? {
        Some(layout) => layout,
        None => {
            // If we run out of gas when charging for failure, we don't want the `OUT_OF_GAS` error
            // to mask the actual error `NFE_BCS_SERIALIZATION_FAILURE`, so we saturate deduction at 0
            context.charge_gas(gas_params.failure)?;
            return Ok(NativeResult::err(
                context.gas_used(),
                NFE_BCS_SERIALIZATION_FAILURE,
            ));
        }
    };
    if let Some(per_input_abstract_unit) = gas_params.per_input_abstract_unit {
        let size = ref_to_val.abstract_memory_size(&SizeConfig {
            traverse_references: true,
            include_vector_size: true,
        })?;
        native_charge_gas_early_exit!(
            context,
            per_input_abstract_unit.saturating_mul(u64::from(size))
        );
    }

    let serialized_value = match ref_to_val {
        Reference::Value(mem_box) => mem_box.try_borrow()?.typed_serialize(&layout),
        indexed @ Reference::Indexed(_) => {
            let value = indexed.read_ref()?;
            safe_assert!(matches!(
                &value,
                Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::U256(_)
                    | Value::Bool(_)
                    | Value::Address(_)
            ));
            value.typed_serialize(&layout)
        }
    };
    let serialized_value = match serialized_value {
        Some(serialized_value) => serialized_value,
        None => {
            // If we run out of gas when charging for failure, we don't want the `OUT_OF_GAS` error
            // to mask the actual error `NFE_BCS_SERIALIZATION_FAILURE`, so we saturate deduction at 0
            context.charge_gas(gas_params.failure)?;
            return Ok(NativeResult::err(
                context.gas_used(),
                NFE_BCS_SERIALIZATION_FAILURE,
            ));
        }
    };
    native_charge_gas_early_exit!(
        context,
        gas_params.per_byte_serialized.saturating_mul(std::cmp::max(
            serialized_value.len() as u64,
            gas_params.legacy_min_output_size,
        ))
    );

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::vector_u8(serialized_value)],
    ))
}

pub fn make_native_to_bytes(gas_params: ToBytesGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            let result = native_to_bytes(&gas_params, context, ty_args, args);
            NativeResult::map_out_of_gas(context.gas_budget(), result)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub to_bytes: ToBytesGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [("to_bytes", make_native_to_bytes(gas_params.to_bytes))];

    make_module_natives(natives)
}
