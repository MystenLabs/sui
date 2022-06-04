// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod event;
mod id;
mod test_scenario;
mod transfer;
mod tx_context;

use move_binary_format::errors::PartialVMError;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};
use move_vm_types::values::{Struct, Value};

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
            "drop_object_for_testing",
            test_scenario::drop_object_for_testing,
        ),
        (
            "TestScenario",
            "emit_wrapped_object_events",
            test_scenario::emit_wrapped_object_events,
        ),
        (
            "TestScenario",
            "get_account_owned_inventory",
            test_scenario::get_account_owned_inventory,
        ),
        (
            "TestScenario",
            "get_object_owned_inventory",
            test_scenario::get_object_owned_inventory,
        ),
        (
            "TestScenario",
            "get_unowned_inventory",
            test_scenario::get_unowned_inventory,
        ),
        ("TestScenario", "num_events", test_scenario::num_events),
        (
            "TestScenario",
            "update_object",
            test_scenario::update_object,
        ),
        (
            "Transfer",
            "delete_child_object_internal",
            transfer::delete_child_object_internal,
        ),
        ("Transfer", "transfer_internal", transfer::transfer_internal),
        ("Transfer", "freeze_object", transfer::freeze_object),
        ("Transfer", "share_object", transfer::share_object),
        ("TxContext", "derive_id", tx_context::derive_id),
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

// Object { id: VersionedID { id: UniqueID { id: ID { bytes: address } } } .. }
// Extract the first field of the struct 4 times to get the id bytes.
pub fn get_object_id(object: Value) -> Result<Value, PartialVMError> {
    get_nested_struct_field(object, &[0, 0, 0, 0])
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
