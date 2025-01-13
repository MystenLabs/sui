// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::nitro_attestation::{parse_nitro_attestation_inner, verify_nitro_attestation_inner};

use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_vm_runtime::native_charge_gas_early_exit;

pub const NOT_SUPPORTED_ERROR: u64 = 0;
pub const PARSE_ERROR: u64 = 1;
pub const VERIFY_ERROR: u64 = 2;
// Gas related structs and functions.

#[derive(Clone)]
pub struct NitroAttestationCostParams {
    pub parse_cost: Option<InternalGas>,
    pub verify_base_cost: Option<InternalGas>,
    pub verify_cost_per_cert: Option<InternalGas>,
}

macro_rules! native_charge_gas_early_exit_option {
    ($native_context:ident, $cost:expr) => {{
        use move_binary_format::errors::PartialVMError;
        use move_core_types::vm_status::StatusCode;
        native_charge_gas_early_exit!(
            $native_context,
            $cost.ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for group ops is missing".to_string())
            })?
        );
    }};
}

fn is_supported(context: &NativeContext) -> bool {
    context
        .extensions()
        .get::<ObjectRuntime>()
        .protocol_config
        .enable_nitro_attestation()
}

pub fn verify_nitro_attestation_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    let current_timestamp = pop_arg!(args, u64);
    let attestation_ref = pop_arg!(args, VectorRef);
    let attestation_bytes = attestation_ref.as_bytes_ref();

    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .nitro_attestation_cost_params
        .clone();

    match parse_nitro_attestation_inner(&attestation_bytes) {
        Ok((signature, signed_message, payload)) => {
            native_charge_gas_early_exit_option!(context, cost_params.parse_cost);
            let cert_chain_length = payload.cabundle.len();
            native_charge_gas_early_exit_option!(
                context,
                cost_params
                    .verify_base_cost
                    .and_then(|base_cost| cost_params
                        .verify_cost_per_cert
                        .map(|per_byte| base_cost + per_byte * (cert_chain_length as u64).into()))
            );
            match verify_nitro_attestation_inner(
                &signature,
                &signed_message,
                &payload,
                current_timestamp,
            ) {
                Ok(()) => Ok(NativeResult::ok(
                    context.gas_used(),
                    smallvec![Value::vector_for_testing_only(
                        payload
                            .pcrs
                            .iter()
                            .map(|pcr| Value::vector_u8(pcr.to_vec()))
                            .collect::<Vec<_>>()
                    )],
                )),
                Err(_) => Ok(NativeResult::err(context.gas_used(), VERIFY_ERROR)),
            }
        }
        Err(_) => Ok(NativeResult::err(context.gas_used(), PARSE_ERROR)),
    }
}
