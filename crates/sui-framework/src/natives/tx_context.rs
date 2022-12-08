// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, convert::TryFrom};
use sui_types::base_types::TransactionDigest;

use crate::{legacy_create_signer_cost, natives::object_runtime::ObjectRuntime};

pub fn derive_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let ids_created = pop_arg!(args, u64);
    let tx_hash = pop_arg!(args, Vec<u8>);

    // TODO(https://github.com/MystenLabs/sui/issues/58): finalize digest format
    // unwrap safe because all digests in Move are serialized from the Rust `TransactionDigest`
    let digest = TransactionDigest::try_from(tx_hash.as_slice()).unwrap();
    let address = AccountAddress::from(digest.derive_id(ids_created));
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.new_id(address.into());

    // TODO: choose cost
    let cost = legacy_create_signer_cost();

    Ok(NativeResult::ok(cost, smallvec![Value::address(address)]))
}
