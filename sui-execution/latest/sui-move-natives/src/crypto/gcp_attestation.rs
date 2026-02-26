// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Struct, Value, Vector, VectorRef, VectorSpecialization},
};
use std::collections::VecDeque;
use sui_types::gcp_attestation::{GcpAttestationDocument, GcpAttestationError, verify_gcp_attestation};

use crate::{NativesCostTable, get_extension, object_runtime::ObjectRuntime};
use move_vm_runtime::native_charge_gas_early_exit;

pub const NOT_SUPPORTED_ERROR: u64 = 0;
pub const PARSE_ERROR: u64 = 1;
pub const VERIFY_ERROR: u64 = 2;

#[derive(Clone)]
pub struct GcpAttestationCostParams {
    pub verify_base_cost: Option<InternalGas>,
    pub verify_cost_per_byte: Option<InternalGas>,
}

macro_rules! native_charge_gas_early_exit_option {
    ($native_context:ident, $cost:expr) => {{
        use move_binary_format::errors::PartialVMError;
        use move_core_types::vm_status::StatusCode;
        native_charge_gas_early_exit!(
            $native_context,
            $cost.ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for GCP attestation is missing".to_string())
            })?
        );
    }};
}

fn is_supported(context: &NativeContext) -> PartialVMResult<bool> {
    Ok(get_extension!(context, ObjectRuntime)?
        .protocol_config
        .enable_gcp_attestation())
}

pub fn verify_gcp_attestation_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 4);

    let cost = context.gas_used();
    if !is_supported(context)? {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    // Pop args in reverse order from the Move call: (token, jwk_n, jwk_e, current_timestamp_ms).
    // Use VectorRef to borrow the Move vectors without moving/destructing them.
    let current_timestamp_ms = pop_arg!(args, u64);
    let jwk_e_ref = pop_arg!(args, VectorRef);
    let jwk_n_ref = pop_arg!(args, VectorRef);
    let token_ref = pop_arg!(args, VectorRef);
    let jwk_e = jwk_e_ref.as_bytes_ref();
    let jwk_n = jwk_n_ref.as_bytes_ref();
    let token = token_ref.as_bytes_ref();

    let cost_params = get_extension!(context, NativesCostTable)?
        .gcp_attestation_cost_params
        .clone();

    native_charge_gas_early_exit_option!(
        context,
        cost_params.verify_base_cost.and_then(|base| {
            cost_params
                .verify_cost_per_byte
                .map(|per_byte| base + per_byte * (token.len() as u64).into())
        })
    );

    match verify_gcp_attestation(&token, &jwk_n, &jwk_e, current_timestamp_ms) {
        Ok(doc) => {
            let result = pack_document(doc);
            NativeResult::map_partial_vm_result_one(context.gas_used(), result)
        }
        Err(GcpAttestationError::ParseError(_)) => {
            Ok(NativeResult::err(context.gas_used(), PARSE_ERROR))
        }
        Err(GcpAttestationError::VerifyError(_)) => {
            Ok(NativeResult::err(context.gas_used(), VERIFY_ERROR))
        }
    }
}

/// Pack a GcpAttestationDocument into a Move struct Value.
/// Field order must match the Move struct definition in gcp_attestation.move.
fn pack_document(doc: GcpAttestationDocument) -> PartialVMResult<Value> {
    Ok(Value::struct_(Struct::pack(vec![
        Value::vector_u8(doc.iss),
        Value::vector_u8(doc.sub),
        Value::vector_u8(doc.aud),
        Value::u64(doc.exp),
        Value::u64(doc.iat),
        to_vector_of_vector_u8(doc.eat_nonce)?,
        Value::bool(doc.secboot),
        Value::vector_u8(doc.hwmodel),
        Value::vector_u8(doc.swname),
        Value::vector_u8(doc.dbgstat),
        to_vector_of_vector_u8(doc.swversion)?,
        Value::vector_u8(doc.image_digest),
        Value::vector_u8(doc.image_reference),
        Value::vector_u8(doc.restart_policy),
    ])))
}

/// Convert a `Vec<Vec<u8>>` into a Move `vector<vector<u8>>` value.
fn to_vector_of_vector_u8(items: Vec<Vec<u8>>) -> PartialVMResult<Value> {
    let inner: Vec<Value> = items.into_iter().map(Value::vector_u8).collect();
    Vector::pack(VectorSpecialization::Container, inner)
}

