// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_emit_cost;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorMetadataV1;

pub fn validate_metadata_bcs(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);
    let metadata_bytes = pop_arg!(args, Vec<u8>);
    let validator_metadata =
        bcs::from_bytes::<ValidatorMetadataV1>(&metadata_bytes).map_err(|_| {
            PartialVMError::new(StatusCode::MALFORMED).with_message(
                "ValidateMetadata Move struct does not much internal ValidateMetadata struct"
                    .to_string(),
            )
        })?;

    // TODO: what should the cost of this be?
    if let Result::Err(err_code) = validator_metadata.verify() {
        return Ok(NativeResult::err(legacy_emit_cost(), err_code));
    }

    let cost = legacy_emit_cost();
    Ok(NativeResult::ok(cost, smallvec![]))
}
