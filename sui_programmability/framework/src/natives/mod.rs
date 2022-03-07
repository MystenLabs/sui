// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod event;
mod id;
mod test_scenario;
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
        ("ID", "delete_id", id::delete_id),
        ("ID", "get_versioned_id", id::get_versioned_id),
        (
            "TestScenario",
            "deleted_object_ids",
            test_scenario::deleted_object_ids,
        ),
        (
            "TestScenario",
            "delete_object_for_testing",
            test_scenario::delete_object_for_testing,
        ),
        (
            "TestScenario",
            "emit_wrapped_object_event",
            test_scenario::emit_wrapped_object_event,
        ),
        (
            "TestScenario",
            "get_inventory",
            test_scenario::get_inventory,
        ),
        ("TestScenario", "num_events", test_scenario::num_events),
        (
            "TestScenario",
            "transferred_object_ids",
            test_scenario::transferred_object_ids,
        ),
        ("Transfer", "transfer_internal", transfer::transfer_internal),
        ("Transfer", "freeze_object", transfer::freeze_object),
        ("TxContext", "fresh_id", tx_context::fresh_id),
        (
            "TxContext",
            "new_signer_from_address",
            tx_context::new_signer_from_address,
        ),
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
