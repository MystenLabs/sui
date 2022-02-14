// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

mod event;
mod id;
mod test_helper;
mod transfer;
mod tx_context;

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};

pub fn all_natives(
    move_stdlib_addr: AccountAddress,
    sui_framework_addr: AccountAddress,
) -> NativeFunctionTable {
    const SUI_NATIVES: &[(&str, &str, NativeFunction)] = &[
        ("Event", "emit", event::emit),
        ("ID", "bytes_to_address", id::bytes_to_address),
        ("ID", "delete", id::delete),
        ("ID", "get_id", id::get_id),
        (
            "TestHelper",
            "get_last_received_object_internal",
            test_helper::get_last_received_object,
        ),
        ("Transfer", "transfer_internal", transfer::transfer_internal),
        (
            "Transfer",
            "transfer_to_object_id",
            transfer::transfer_to_object_id,
        ),
        ("TxContext", "fresh_id", tx_context::fresh_id),
    ];
    SUI_NATIVES
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
        .chain(move_stdlib::natives::all_natives(move_stdlib_addr))
        .collect()
}
