// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::NativesCostTable;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{gas_algebra::InternalGas, vm_status::StatusCode};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorMetadataV1;

#[derive(Clone, Debug)]
pub struct ValidatorValidateMetadataBcsCostParams {
    pub validator_validate_metadata_cost_base: InternalGas,
    pub validator_validate_metadata_data_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun validate_metadata_bcs
 * Implementation of the Move native function `validate_metadata_bcs(metadata: vector<u8>)`
 *   gas cost: validator_validate_metadata_cost_base           | fixed cosrs
 *              + validator_validate_metadata_data_cost_per_byte * metadata_bytes.len()   | assume cost is proportional to size
 **************************************************************************************************/
pub fn validate_metadata_bcs(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let validator_validate_metadata_bcs_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .validator_validate_metadata_bcs_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        validator_validate_metadata_bcs_cost_params.validator_validate_metadata_cost_base
    );

    let metadata_bytes = pop_arg!(args, Vec<u8>);

    native_charge_gas_early_exit!(
        context,
        validator_validate_metadata_bcs_cost_params.validator_validate_metadata_data_cost_per_byte
            * (metadata_bytes.len() as u64).into()
    );

    let validator_metadata =
        bcs::from_bytes::<ValidatorMetadataV1>(&metadata_bytes).map_err(|_| {
            PartialVMError::new(StatusCode::MALFORMED).with_message(
                "ValidateMetadata Move struct does not match internal ValidateMetadata struct"
                    .to_string(),
            )
        })?;

    let cost = context.gas_used();

    if let Result::Err(err_code) = validator_metadata.verify() {
        return Ok(NativeResult::err(cost, err_code));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
