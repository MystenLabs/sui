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
use sui_types::attestation::attestation_verify_inner;
pub fn nitro_attestation_verify_inner(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 6);

    // todo: figure out cost

    let timestamp = pop_arg!(args, u64);
    let pcr2 = pop_arg!(args, VectorRef);
    let pcr1 = pop_arg!(args, VectorRef);
    let pcr0 = pop_arg!(args, VectorRef);
    let enclave_pk = pop_arg!(args, VectorRef);
    let attestation = pop_arg!(args, VectorRef);

    let attestation_ref = attestation.as_bytes_ref();
    let enclave_pk_ref = enclave_pk.as_bytes_ref();
    let pcr0_ref = pcr0.as_bytes_ref();
    let pcr1_ref = pcr1.as_bytes_ref();
    let pcr2_ref = pcr2.as_bytes_ref();

    if attestation_verify_inner(
        &attestation_ref,
        &enclave_pk_ref,
        &pcr0_ref,
        &pcr1_ref,
        &pcr2_ref,
        timestamp,
    )
    .is_ok()
    {
        Ok(NativeResult::ok(
            InternalGas::zero(),
            smallvec![Value::bool(true)],
        ))
    } else {
        Ok(NativeResult::ok(
            InternalGas::zero(),
            smallvec![Value::bool(false)],
        ))
    }
}
