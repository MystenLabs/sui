// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::serde_helpers::ToFromByteArray;
use std::collections::VecDeque;
use fastcrypto::bulletproofs::{Range, RangeProof};
use fastcrypto::error::FastCryptoError::InvalidInput;
use fastcrypto::error::FastCryptoResult;
use fastcrypto::groups::ristretto255::RistrettoPoint;
use fastcrypto::pedersen::PedersenCommitment;
use rand::thread_rng;
use smallvec::smallvec;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::loaded_data::runtime_types::Type;
use move_vm_types::natives::function::NativeResult;
use move_vm_types::pop_arg;
use move_vm_types::values::{Value, VectorRef};
use crate::{get_extension, NativesCostTable};

pub const INVALID_PROOF: u64 = 0;
pub const INVALID_COMMITMENT: u64 = 1;
pub const INVALID_RANGE: u64 = 2;

#[derive(Clone)]
pub struct VerifyBulletproofRistretto255CostParams {
    /// Base cost for invoking the `hmac_sha3_256` function
    pub verify_bulletproof_ristretto255_cost: Option<InternalGas>,
}

pub fn verify_bulletproof_ristretto255(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    // Load the cost parameters from the protocol config
    let verify_bulletproof_ristretto255_cost_params = get_extension!(context, NativesCostTable)?
        .verify_bulletproof_ristretto255_cost_params
        .clone();

    // Charge the base cost for this operation
/*    native_charge_gas_early_exit!(
        context,
        verify_bulletproof_ristretto255_cost_params.verify_bulletproof_ristretto255_cost
    );*/

    let commitment = pop_arg!(args, VectorRef);
    let range = pop_arg!(args, u8);
    let proof = pop_arg!(args, VectorRef);

    let Ok(commitment) = commitment.as_bytes_ref().to_vec().try_into().map_err(|_| InvalidInput).and_then(|b| RistrettoPoint::from_byte_array(&b)).map(PedersenCommitment) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_COMMITMENT));
    };

    let Ok(range) = range_from_bits(range).map_err(|_| InvalidInput) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_RANGE));
    };

    let Ok(proof) = bcs::from_bytes::<RangeProof>(&proof.as_bytes_ref()) else {
        return Ok(NativeResult::err(context.gas_used(), INVALID_PROOF));
    };

    let result = proof.verify(&commitment, &range, &mut thread_rng());

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(result.is_ok())]
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