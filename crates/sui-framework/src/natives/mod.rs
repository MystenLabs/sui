// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod crypto;
mod event;
mod object;
mod test_scenario;
mod transfer;
mod tx_context;
mod types;

use crate::make_native;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_stdlib::natives::GasParameters;
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};
use move_vm_types::{
    natives::function::NativeResult,
    values::{Struct, Value},
};
use std::sync::Arc;

pub fn all_natives(
    move_stdlib_addr: AccountAddress,
    sui_framework_addr: AccountAddress,
) -> NativeFunctionTable {
    let sui_natives: &[(&str, &str, NativeFunction)] = &[
        ("crypto", "ecrecover", make_native!(crypto::ecrecover)),
        ("crypto", "keccak256", make_native!(crypto::keccak256)),
        (
            "crypto",
            "secp256k1_verify",
            make_native!(crypto::secp256k1_verify),
        ),
        (
            "crypto",
            "bls12381_verify_g1_sig",
            make_native!(crypto::bls12381_verify_g1_sig),
        ),
        ("event", "emit", make_native!(event::emit)),
        (
            "object",
            "bytes_to_address",
            make_native!(object::bytes_to_address),
        ),
        ("object", "delete_impl", make_native!(object::delete_impl)),
        ("object", "borrow_uid", make_native!(object::borrow_uid)),
        (
            "crypto",
            "native_verify_full_range_proof",
            make_native!(crypto::verify_range_proof),
        ),
        (
            "elliptic_curve",
            "native_add_ristretto_point",
            make_native!(crypto::add_ristretto_point),
        ),
        (
            "elliptic_curve",
            "native_subtract_ristretto_point",
            make_native!(crypto::subtract_ristretto_point),
        ),
        (
            "elliptic_curve",
            "native_create_pedersen_commitment",
            make_native!(crypto::pedersen_commit),
        ),
        (
            "elliptic_curve",
            "native_scalar_from_u64",
            make_native!(crypto::scalar_from_u64),
        ),
        (
            "elliptic_curve",
            "native_scalar_from_bytes",
            make_native!(crypto::scalar_from_bytes),
        ),
        (
            "crypto",
            "ed25519_verify",
            make_native!(crypto::ed25519_verify),
        ),
        (
            "test_scenario",
            "drop_object_for_testing",
            make_native!(test_scenario::drop_object_for_testing),
        ),
        (
            "test_scenario",
            "emit_wrapped_object_events",
            make_native!(test_scenario::emit_wrapped_object_events),
        ),
        (
            "test_scenario",
            "get_account_owned_inventory",
            make_native!(test_scenario::get_account_owned_inventory),
        ),
        (
            "test_scenario",
            "get_object_owned_inventory",
            make_native!(test_scenario::get_object_owned_inventory),
        ),
        (
            "test_scenario",
            "get_unowned_inventory",
            make_native!(test_scenario::get_unowned_inventory),
        ),
        (
            "test_scenario",
            "num_events",
            make_native!(test_scenario::num_events),
        ),
        (
            "test_scenario",
            "update_object",
            make_native!(test_scenario::update_object),
        ),
        (
            "transfer",
            "transfer_internal",
            make_native!(transfer::transfer_internal),
        ),
        (
            "transfer",
            "freeze_object",
            make_native!(transfer::freeze_object),
        ),
        (
            "transfer",
            "share_object",
            make_native!(transfer::share_object),
        ),
        (
            "tx_context",
            "derive_id",
            make_native!(tx_context::derive_id),
        ),
        (
            "tx_context",
            "new_signer_from_address",
            make_native!(tx_context::new_signer_from_address),
        ),
        (
            "types",
            "is_one_time_witness",
            make_native!(types::is_one_time_witness),
        ),
    ];
    sui_natives
        .iter()
        .cloned()
        .map(|(module_name, func_name, func)| {
            (
                sui_framework_addr,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .chain(move_stdlib::natives::all_natives(
            move_stdlib_addr,
            // TODO: tune gas params
            GasParameters::zeros(),
        ))
        .collect()
}

// Object { info: Info { id: ID { bytes: address } } .. }
// Extract the first field of the struct 3 times to get the id bytes.
pub fn get_object_id(object: Value) -> Result<Value, PartialVMError> {
    get_nested_struct_field(object, &[0, 0, 0])
}

// Extract a field valye that's nested inside value `v`. The offset of each nesting
// is determined by `offsets`.
pub fn get_nested_struct_field(mut v: Value, offsets: &[usize]) -> Result<Value, PartialVMError> {
    for offset in offsets {
        v = get_nth_struct_field(v, *offset)?;
    }
    Ok(v)
}

pub fn get_nth_struct_field(v: Value, n: usize) -> Result<Value, PartialVMError> {
    let mut itr = v.value_as::<Struct>()?.unpack()?;
    Ok(itr.nth(n).unwrap())
}

#[macro_export]
macro_rules! make_native {
    ($native: expr) => {
        Arc::new(
            move |context, ty_args, args| -> PartialVMResult<NativeResult> {
                $native(context, ty_args, args)
            },
        )
    };
}
