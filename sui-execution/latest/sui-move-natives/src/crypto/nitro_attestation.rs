// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{gas_algebra::InternalGas, vm_status::StatusCode};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Struct, Value, Vector, VectorRef},
};
use std::collections::VecDeque;
use sui_types::nitro_attestation::{parse_nitro_attestation, verify_nitro_attestation};

use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_vm_runtime::native_charge_gas_early_exit;

pub const NOT_SUPPORTED_ERROR: u64 = 0;
pub const PARSE_ERROR: u64 = 1;
pub const VERIFY_ERROR: u64 = 2;
pub const INVALID_PCRS_ERROR: u64 = 3;

// Gas related structs and functions.
#[derive(Clone)]
pub struct NitroAttestationCostParams {
    pub parse_base_cost: Option<InternalGas>,
    pub parse_cost_per_byte: Option<InternalGas>,
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
                    .with_message("Gas cost for nitro attestation is missing".to_string())
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

pub fn load_nitro_attestation_internal(
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

    native_charge_gas_early_exit_option!(
        context,
        cost_params.parse_base_cost.and_then(|base_cost| cost_params
            .parse_cost_per_byte
            .map(|per_byte| base_cost + per_byte * (attestation_bytes.len() as u64).into()))
    );
    match parse_nitro_attestation(&attestation_bytes) {
        Ok((signature, signed_message, payload)) => {
            let cert_chain_length = payload.get_cert_chain_length();
            native_charge_gas_early_exit_option!(
                context,
                cost_params
                    .verify_base_cost
                    .and_then(|base_cost| cost_params
                        .verify_cost_per_cert
                        .map(|per_cert| base_cost + per_cert * (cert_chain_length as u64).into()))
            );
            match verify_nitro_attestation(&signature, &signed_message, &payload, current_timestamp)
            {
                Ok(()) => {
                    // Encapsulate as a lambda and call to allow us to capture any `Err` returns.
                    // Could do this with `and_then` as well if desired.
                    let result = || {
                        Ok(Value::struct_(Struct::pack(vec![
                            Value::vector_u8(payload.module_id.as_bytes().to_vec()),
                            Value::u64(payload.timestamp),
                            Value::vector_u8(payload.digest.as_bytes().to_vec()),
                            to_indexed_struct(payload.pcrs)?,
                            to_option_vector_u8(payload.public_key)?,
                            to_option_vector_u8(payload.user_data)?,
                            to_option_vector_u8(payload.nonce)?,
                        ])))
                    };
                    NativeResult::map_partial_vm_result_one(context.gas_used(), result())
                }
                Err(_) => Ok(NativeResult::err(context.gas_used(), VERIFY_ERROR)),
            }
        }
        Err(_) => Ok(NativeResult::err(context.gas_used(), PARSE_ERROR)),
    }
}

// Build an Option<vector<u8>> value
fn to_option_vector_u8(value: Option<Vec<u8>>) -> PartialVMResult<Value> {
    let vector_u8_type = Type::Vector(Box::new(Type::U8));
    match value {
        // Some(<vector<u8>>) = { vector[ <vector<u8>> ] }
        Some(vec) => Ok(Value::struct_(Struct::pack(vec![Vector::pack(
            &vector_u8_type,
            vec![Value::vector_u8(vec)],
        )?]))),
        // None = { vector[ ] }
        None => Ok(Value::struct_(Struct::pack(vec![Vector::empty(
            &vector_u8_type,
        )?]))),
    }
}

// Convert a list of PCRs into a vector of PCREntry struct with index and value,
// where the indices are [0, 1, 2, 3, 4, 8] since AWS currently supports PCR0,
// PCR1, PCR2, PCR3, PCR4, PCR8.
fn to_indexed_struct(pcrs: Vec<Vec<u8>>) -> PartialVMResult<Value> {
    let indices = [0, 1, 2, 3, 4, 8];
    if pcrs.len() != indices.len() {
        return Err(PartialVMError::new(StatusCode::ABORTED).with_sub_status(INVALID_PCRS_ERROR));
    };
    let mut indexed_struct = vec![];
    for (index, pcr) in pcrs.iter().enumerate() {
        indexed_struct.push(Value::struct_(Struct::pack(vec![
            Value::u8(indices[index]),
            Value::vector_u8(pcr.to_vec()),
        ])));
    }
    Ok(Value::vector_for_testing_only(indexed_struct))
}
