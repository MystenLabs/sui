// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::NativesCostTable;
use fastcrypto::error::FastCryptoError;
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::u256::U256;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::natives::function::PartialVMError;
use move_vm_types::values::VectorRef;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const INVALID_INPUT: u64 = 0;

#[derive(Clone)]
pub struct CheckZkloginIdCostParams {
    /// Base cost for invoking the `check_zklogin_id` function
    pub check_zklogin_id_cost_base: Option<InternalGas>,
}
/***************************************************************************************************
 * native fun check_zklogin_id
 * Implementation of the Move native function `zklogin::check_zklogin_id(address: &address, name: &String, value: &String, iss: &String, aud: &String, pin_hash: &u256): bool;`
 *   gas cost: check_zklogin_id_cost | The values name, value, iss and aud are hashed as part of this function, but their sizes are bounded from above, so we may assume that the cost is constant.
 **************************************************************************************************/
pub fn check_zklogin_id_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Load the cost parameters from the protocol config
    let check_zklogin_id_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .check_zklogin_id_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        check_zklogin_id_cost_params
            .check_zklogin_id_cost_base
            .ok_or(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for check_zklogin_id not available".to_string())
            )?
    );

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 6);

    // Poseidon hash of the user's pin code
    let pin_hash = pop_arg!(args, U256);

    // The audience (wallet) id
    let aud = pop_arg!(args, VectorRef);

    // The issuer (identity provider) id
    let iss = pop_arg!(args, VectorRef);

    // The claim value (sub, email, etc)
    let kc_value = pop_arg!(args, VectorRef);

    // The claim name (sub, email, etc)
    let kc_name = pop_arg!(args, VectorRef);

    // The address to check
    let address = pop_arg!(args, AccountAddress);

    let result = verify_zk_login_id_internal(
        &address,
        &kc_name.as_bytes_ref(),
        &kc_value.as_bytes_ref(),
        &aud.as_bytes_ref(),
        &iss.as_bytes_ref(),
        &pin_hash,
    );

    match result {
        Ok(result) => Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::bool(result)],
        )),
        Err(_) => Ok(NativeResult::err(context.gas_used(), INVALID_INPUT)),
    }
}

fn verify_zk_login_id_internal(
    address: &AccountAddress,
    kc_name: &[u8],
    kc_value: &[u8],
    aud: &[u8],
    iss: &[u8],
    pin_hash: &U256,
) -> Result<bool, FastCryptoError> {
    match fastcrypto_zkp::bn254::zk_login_api::verify_zk_login_id(
        &address.into_bytes(),
        std::str::from_utf8(kc_name).map_err(|_| FastCryptoError::InvalidInput)?,
        std::str::from_utf8(kc_value).map_err(|_| FastCryptoError::InvalidInput)?,
        std::str::from_utf8(aud).map_err(|_| FastCryptoError::InvalidInput)?,
        std::str::from_utf8(iss).map_err(|_| FastCryptoError::InvalidInput)?,
        &pin_hash.to_string(),
    ) {
        Ok(_) => Ok(true),
        Err(FastCryptoError::InvalidProof) => Ok(false),
        Err(_) => Err(FastCryptoError::InvalidInput),
    }
}

#[derive(Clone)]
pub struct CheckZkloginIssCostParams {
    /// Base cost for invoking the `check_zklogin_iss` function
    pub check_zklogin_iss_cost_base: Option<InternalGas>,
}
/***************************************************************************************************
 * native fun check_zklogin_iss
 * Implementation of the Move native function `zklogin::check_zklogin_iss(address: &address, iss: &String, address_seed: &u256): bool;`
 *   gas cost: check_zklogin_iss_cost | The iss value is hashed as part of this function, but its size is bounded from above so we may assume that the cost is constant.
 **************************************************************************************************/
pub fn check_zklogin_iss_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Load the cost parameters from the protocol config
    let check_zklogin_id_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .check_zklogin_iss_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        check_zklogin_id_cost_params
            .check_zklogin_iss_cost_base
            .ok_or(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for check_zklogin_iss not available".to_string())
            )?
    );

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    // The issuer (identity provider) id
    let iss = pop_arg!(args, VectorRef);

    // The audience (wallet) id
    let address_seed = pop_arg!(args, U256);

    // The address to check
    let address = pop_arg!(args, AccountAddress);

    let result = verify_zk_login_iss_internal(&address, &address_seed, &iss.as_bytes_ref());

    match result {
        Ok(result) => Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::bool(result)],
        )),
        Err(_) => Ok(NativeResult::err(context.gas_used(), INVALID_INPUT)),
    }
}

fn verify_zk_login_iss_internal(
    address: &AccountAddress,
    address_seed: &U256,
    iss: &[u8],
) -> Result<bool, FastCryptoError> {
    match fastcrypto_zkp::bn254::zk_login_api::verify_zk_login_iss(
        &address.into_bytes(),
        &address_seed.to_string(),
        std::str::from_utf8(iss).map_err(|_| FastCryptoError::InvalidInput)?,
    ) {
        Ok(_) => Ok(true),
        Err(FastCryptoError::InvalidProof) => Ok(false),
        Err(_) => Err(FastCryptoError::InvalidInput),
    }
}
