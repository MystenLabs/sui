// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NativesCostTable, get_extension, object_runtime::ObjectRuntime};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{gas_algebra::InternalGas, vm_status::StatusCode};
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::natives::functions::NativeContext;
use move_vm_runtime::{
    execution::{
        Type,
        values::{Value, VectorRef},
    },
    natives::functions::NativeResult,
    pop_arg,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::ecdsa_p384::{P384Hash, verify_secp384r1};

pub const SHA256: u8 = 0;
pub const SHA384: u8 = 1;
pub const NOT_SUPPORTED_ERROR: u64 = 0;

// Input block sizes for the supported hash functions, used to price message hashing per block.
const SHA256_BLOCK_SIZE: usize = 64;
const SHA384_BLOCK_SIZE: usize = 128;

#[derive(Clone)]
pub struct EcdsaP384Secp384R1VerifyCostParams {
    pub ecdsa_p384_secp384r1_verify_sha256_cost_base: Option<InternalGas>,
    pub ecdsa_p384_secp384r1_verify_sha256_msg_cost_per_byte: Option<InternalGas>,
    pub ecdsa_p384_secp384r1_verify_sha256_msg_cost_per_block: Option<InternalGas>,
    pub ecdsa_p384_secp384r1_verify_sha384_cost_base: Option<InternalGas>,
    pub ecdsa_p384_secp384r1_verify_sha384_msg_cost_per_byte: Option<InternalGas>,
    pub ecdsa_p384_secp384r1_verify_sha384_msg_cost_per_block: Option<InternalGas>,
}

macro_rules! native_charge_gas_early_exit_option {
    ($native_context:ident, $cost:expr) => {{
        native_charge_gas_early_exit!(
            $native_context,
            $cost.ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for ecdsa_p384 is missing".to_string())
            })?
        );
    }};
}

fn is_supported(context: &NativeContext) -> PartialVMResult<bool> {
    Ok(get_extension!(context, ObjectRuntime)?
        .protocol_config
        .enable_ecdsa_p384_native())
}

/***************************************************************************************************
 * native fun secp384r1_verify
 * Implementation of the Move native function
 * `secp384r1_verify(signature: &vector<u8>, public_key: &vector<u8>, msg: &vector<u8>, hash: u8): bool`
 * This function has two cost modes depending on the hash being `SHA256` or `SHA384`. The core
 * formula is the same but the constants differ. The fixed-size signature and public key are
 * covered by the base cost; only the variable-length `msg` is priced per byte and per block.
 **************************************************************************************************/
pub fn secp384r1_verify(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 4);

    if !is_supported(context)? {
        return Ok(NativeResult::err(context.gas_used(), NOT_SUPPORTED_ERROR));
    }

    let (cost_params, crypto_invalid_arguments_cost) = {
        let cost_table = get_extension!(context, NativesCostTable)?;
        (
            cost_table.ecdsa_p384_secp384_r1_verify_cost_params.clone(),
            cost_table.crypto_invalid_arguments_cost,
        )
    };

    let hash = pop_arg!(args, u8);
    let (base_cost, cost_per_byte, cost_per_block, block_size, p384_hash) = match hash {
        SHA256 => (
            cost_params.ecdsa_p384_secp384r1_verify_sha256_cost_base,
            cost_params.ecdsa_p384_secp384r1_verify_sha256_msg_cost_per_byte,
            cost_params.ecdsa_p384_secp384r1_verify_sha256_msg_cost_per_block,
            SHA256_BLOCK_SIZE,
            P384Hash::Sha256,
        ),
        SHA384 => (
            cost_params.ecdsa_p384_secp384r1_verify_sha384_cost_base,
            cost_params.ecdsa_p384_secp384r1_verify_sha384_msg_cost_per_byte,
            cost_params.ecdsa_p384_secp384r1_verify_sha384_msg_cost_per_block,
            SHA384_BLOCK_SIZE,
            P384Hash::Sha384,
        ),
        _ => {
            // Charge for the failed call but don't propagate out-of-gas, so the real
            // (invalid hash flag) result is not masked by an OUT_OF_GAS error.
            context.charge_gas(crypto_invalid_arguments_cost)?;
            return Ok(NativeResult::ok(
                context.gas_used(),
                smallvec![Value::bool(false)],
            ));
        }
    };

    native_charge_gas_early_exit_option!(context, base_cost);

    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref()?;
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref()?;
    let signature_bytes_ref = signature_bytes.as_bytes_ref()?;

    native_charge_gas_early_exit_option!(
        context,
        cost_per_byte
            .zip(cost_per_block)
            .map(|(per_byte, per_block)| {
                per_byte * (msg_ref.len() as u64).into()
                    + per_block * (msg_ref.len().div_ceil(block_size) as u64).into()
            })
    );

    let cost = context.gas_used();
    let result = verify_secp384r1(
        &signature_bytes_ref,
        &public_key_bytes_ref,
        &msg_ref,
        p384_hash,
    )
    .is_ok();

    Ok(NativeResult::ok(cost, smallvec![Value::bool(result)]))
}
