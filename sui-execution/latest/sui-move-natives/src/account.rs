// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use sui_types::{TypeTag, SUI_FRAMEWORK_ADDRESS};
use tracing::instrument;

use crate::object_runtime::ObjectRuntime;

/***************************************************************************************************
 * native fun hash_type_and_key
 * Implementation of the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
 *   gas cost: dynamic_field_hash_type_and_key_cost_base                            | covers various fixed costs in the oper
 *              + dynamic_field_hash_type_and_key_type_cost_per_byte * size_of(K)   | covers cost of operating on the type `K`
 *              + dynamic_field_hash_type_and_key_value_cost_per_byte * size_of(k)  | covers cost of operating on the value `k`
 *              + dynamic_field_hash_type_and_key_type_tag_cost_per_byte * size_of(type_tag(k))    | covers cost of operating on the type tag of `K`
 **************************************************************************************************/
#[instrument(level = "trace", skip_all, err)]
pub fn emit_account_event(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 1);
    assert_eq!(args.len(), 1);

    // TODO: gas

    let value_ty = ty_args.pop().unwrap();

    let value: Value = args.pop_back().unwrap();

    let tag = match context.type_to_type_tag(&value_ty)? {
        TypeTag::Struct(s) => {
            // emit_account_event is private to account.move, so there should be
            // no possibility of a bad
            assert_eq!(s.address, SUI_FRAMEWORK_ADDRESS);
            assert_eq!(s.module.as_str(), "account");
            assert!(s.name.as_str() == "Merge" || s.name.as_str() == "Split");
            s
        }
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();

    obj_runtime.emit_event(value_ty, *tag, value)?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
