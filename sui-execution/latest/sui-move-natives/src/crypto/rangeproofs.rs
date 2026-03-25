// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_runtime::ObjectRuntime;
use crate::{NativesCostTable, get_extension};
use fastcrypto::bulletproofs::{Range, RangeProof};
use fastcrypto::error::FastCryptoError::InvalidInput;
use fastcrypto::error::FastCryptoResult;
use fastcrypto::groups::ristretto255::RistrettoPoint;
use fastcrypto::pedersen::PedersenCommitment;
use fastcrypto::serde_helpers::ToFromByteArray;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::gas_algebra::InternalGas;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::execution::Type;
use move_vm_runtime::execution::values::{Value, VectorRef};
use move_vm_runtime::natives::functions::{NativeContext, NativeResult};
use move_vm_runtime::{native_charge_gas_early_exit, pop_arg};
use rand::thread_rng;
use smallvec::smallvec;
use std::collections::VecDeque;

pub const NOT_SUPPORTED: u64 = 0;
pub const INVALID_PROOF: u64 = 1;
pub const INVALID_COMMITMENT: u64 = 2;
pub const INVALID_RANGE: u64 = 3;
pub const INVALID_BATCH_SIZE: u64 = 4;

/// Upper bound for batch size * range in bits
pub const MAX_TOTAL_BITS: u64 = 256;

#[derive(Clone)]
pub struct BulletproofsCostParams {
    pub verify_bulletproofs_ristretto255_base_cost: Option<InternalGas>,
    // The performance depends on the number of values/commitments * bits in range
    pub verify_bulletproofs_ristretto255_cost_per_bit: Option<InternalGas>,
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
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("verify_bulletproofs_ristretto255_base_cost not available".to_string())
            )?
    );

    let commitments = pop_arg!(args, VectorRef);
    let range_bits = pop_arg!(args, u8);
    let proof = pop_arg!(args, VectorRef);

    let Ok(proof) = bcs::from_bytes::<RangeProof>(&proof.as_bytes_ref()?) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_PROOF));
    };

    let Ok(range) = range_from_bits(range_bits).map_err(|_| InvalidInput) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_RANGE));
    };

    let length = commitments
        .len(&Type::Vector(Box::new(Type::U8)))?
        .value_as::<u64>()?;

    // The performance depends is linear in length times range bits
    let total_bits = length * range_bits as u64;
    if length == 0 || !length.is_power_of_two() || total_bits > MAX_TOTAL_BITS {
        return Ok(NativeResult::err(context.gas_used(), INVALID_BATCH_SIZE));
    }

    // Charge the message dependent cost
    native_charge_gas_early_exit!(
        context,
        cost_parameters
            .verify_bulletproofs_ristretto255_cost_per_bit
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "verify_bulletproofs_ristretto255_cost_per_bit not available".to_string()
                )
            )?
            * total_bits.into()
    );

    let commitment_bytes = (0..length)
        .map(|i| {
            commitments
                .borrow_elem(i as usize, &Type::Vector(Box::new(Type::U8)))
                .and_then(|reference| reference.value_as::<VectorRef>())
                .and_then(|v| Ok(v.as_bytes_ref()?.to_vec()))
        })
        .collect::<PartialVMResult<Vec<Vec<u8>>>>()?;

    let Ok(commitments) = commitment_bytes
        .into_iter()
        .map(|b| {
            b.try_into()
                .map_err(|_| InvalidInput)
                .and_then(|b| RistrettoPoint::from_byte_array(&b))
                .map(PedersenCommitment)
        })
        .collect::<FastCryptoResult<Vec<PedersenCommitment>>>()
    else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_COMMITMENT));
    };

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
