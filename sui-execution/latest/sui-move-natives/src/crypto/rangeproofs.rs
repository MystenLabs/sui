// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_runtime::ObjectRuntime;
use crate::{NativesCostTable, get_extension};
use fastcrypto::bulletproofs::{Range, RangeProof};
use fastcrypto::error::FastCryptoError::InvalidInput;
use fastcrypto::error::FastCryptoResult;
use fastcrypto::groups::FromTrustedByteArray;
use fastcrypto::groups::ristretto255::RistrettoPoint;
use fastcrypto::pedersen::PedersenCommitment;
use move_binary_format::errors::PartialVMResult;
use move_binary_format::partial_vm_error;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::execution::Type;
use move_vm_runtime::execution::values::{Value, VectorRef};
use move_vm_runtime::natives::functions::{NativeContext, NativeResult};
use move_vm_runtime::{native_charge_gas_early_exit, pop_arg};
use rand::thread_rng;
use smallvec::smallvec;
use std::collections::VecDeque;

pub const NOT_SUPPORTED: u64 = 0;
pub const INVALID_PROOF: u64 = 1;
pub const INVALID_RANGE: u64 = 2;
pub const INVALID_BATCH_SIZE: u64 = 3;

/// Upper bound for batch size * range in bits
pub const MAX_TOTAL_BITS: u64 = 512;

/// Proofs with MAX_TOTAL_BITS = 512 will be exactly 864 bytes.
const MAX_PROOF_SIZE: usize = 864;

#[derive(Clone)]
pub struct BulletproofsCostParams {
    pub verify_bulletproofs_ristretto255_base_cost: Option<InternalGas>,
    // The performance depends on the number of values/commitments * bits in range
    pub verify_bulletproofs_ristretto255_cost_per_bit_and_commitment: Option<InternalGas>,
}

fn is_supported(context: &NativeContext) -> PartialVMResult<bool> {
    Ok(get_extension!(context, ObjectRuntime)?
        .protocol_config
        .enable_verify_bulletproofs_ristretto255())
}

pub fn verify_bulletproofs_ristretto255(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    if !is_supported(context)? {
        return Ok(NativeResult::err(context.gas_used(), NOT_SUPPORTED));
    }

    // Load the cost parameters from the protocol config
    let cost_parameters = get_extension!(context, NativesCostTable)?
        .bulletproofs_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        cost_parameters
            .verify_bulletproofs_ristretto255_base_cost
            .ok_or_else(|| partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "verify_bulletproofs_ristretto255_base_cost not available",
            ))?
    );

    let commitments = pop_arg!(args, VectorRef);
    let range_bits = pop_arg!(args, u8);
    let proof = pop_arg!(args, VectorRef);

    let proof_bytes = proof.as_bytes_ref()?;
    if proof_bytes.len() > MAX_PROOF_SIZE {
        return Ok(NativeResult::err(context.gas_used(), INVALID_PROOF));
    }

    let Ok(proof) = RangeProof::from_bytes(&proof_bytes) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_PROOF));
    };

    let Ok(range) = range_from_bits(range_bits).map_err(|_| InvalidInput) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_RANGE));
    };

    let vector_u8_type = Type::Vector(Box::new(Type::U8));
    let length = commitments.len(&vector_u8_type)?.value_as::<u64>()?;

    // The performance is linear in the product of length and range bits, so it is computed as base_cost + cost_per_bit * length * range_bits.
    let total_bits = length * range_bits as u64;
    if length == 0 || !length.is_power_of_two() || total_bits > MAX_TOTAL_BITS {
        return Ok(NativeResult::err(context.gas_used(), INVALID_BATCH_SIZE));
    }

    // Charge the message dependent cost
    native_charge_gas_early_exit!(
        context,
        cost_parameters
            .verify_bulletproofs_ristretto255_cost_per_bit_and_commitment
            .ok_or_else(|| partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "verify_bulletproofs_ristretto255_cost_per_bit_and_commitment not available",
            ))?
            * total_bits.into()
    );

    let commitments = (0..length)
        .map(|i| {
            commitments
                .borrow_elem(i as usize, &vector_u8_type)
                .and_then(|reference| reference.value_as::<VectorRef>())
                .and_then(|v| Ok(v.as_bytes_ref()?.to_vec()))
                .and_then(|v| {
                    v.try_into()
                        .map_err(|_| partial_vm_error!(INTERNAL_TYPE_ERROR))
                })
                .and_then(|b| {
                    RistrettoPoint::from_trusted_byte_array(&b)
                        .map_err(|_| partial_vm_error!(INTERNAL_TYPE_ERROR))
                })
                .map(PedersenCommitment)
        })
        .collect::<PartialVMResult<Vec<PedersenCommitment>>>()?;

    let result = proof.verify_batch(&commitments, &range, &mut thread_rng());

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(result.is_ok())],
    ))
}

fn range_from_bits(bits: u8) -> FastCryptoResult<Range> {
    match bits {
        8 => Ok(Range::Bits8),
        16 => Ok(Range::Bits16),
        32 => Ok(Range::Bits32),
        64 => Ok(Range::Bits64),
        _ => Err(InvalidInput),
    }
}
