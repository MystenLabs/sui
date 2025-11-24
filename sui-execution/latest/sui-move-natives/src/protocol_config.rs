// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::get_extension;
use crate::object_runtime::ObjectRuntime;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, Vector},
};
use smallvec::smallvec;
use std::collections::VecDeque;

/***************************************************************************************************
 * native fun is_feature_enabled
 *
 * Implementation of the Move native function `protocol_config::is_feature_enabled(
 *      feature_flag_name: vector<u8>): bool`
 *
 * Checks if a protocol feature flag is enabled in the current protocol version.
 *
 * Gas cost: 0 (zero cost for framework-internal use)
 **************************************************************************************************/
pub fn is_feature_enabled(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let feature_flag_name_bytes = pop_arg!(args, Vector);
    let bytes = feature_flag_name_bytes.to_vec_u8()?;

    let feature_flag_name = match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            debug_assert!(false);
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Feature flag name is not a valid UTF-8 string".to_owned()),
            );
        }
    };

    let protocol_config = &get_extension!(context, ObjectRuntime)?.protocol_config;

    // Use the auto-generated lookup_feature method to find the feature flag
    let is_enabled = match protocol_config.lookup_feature(feature_flag_name) {
        Some(value) => value,
        None => {
            debug_assert!(false);
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Feature flag not found".to_owned()),
            );
        }
    };

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(is_enabled)],
    ))
}
