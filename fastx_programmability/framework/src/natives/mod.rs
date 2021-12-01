// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

mod authenticator;
mod transfer;

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};

pub fn all_natives(
    move_stdlib_addr: AccountAddress,
    fastx_framework_addr: AccountAddress,
) -> NativeFunctionTable {
    const FASTX_NATIVES: &[(&str, &str, NativeFunction)] = &[
        (
            "Authenticator",
            "bytes_to_address",
            authenticator::bytes_to_address,
        ),
        ("Transfer", "transfer_internal", transfer::transfer_internal),
    ];
    FASTX_NATIVES
        .iter()
        .cloned()
        .map(|(module_name, func_name, func)| {
            (
                fastx_framework_addr,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .chain(move_stdlib::natives::all_natives(move_stdlib_addr))
        .collect()
}
