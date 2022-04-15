// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod event;
mod id;
mod test_scenario;
mod transfer;
mod tx_context;

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_runtime::native_functions::{NativeContext, NativeFunction, NativeFunctionTable};
use move_vm_types::values::{Struct, Value};
use num_enum::TryFromPrimitive;

use crate::EventType;

use self::test_scenario::TEST_START_EVENT;

/// Trying to transfer or delete a shared object, which is not allowed.
pub const ECONSUME_SHARED_OBJECT: u64 = 100;

#[macro_export(local_inner_macros)]
macro_rules! abort_if_object_shared {
    ($context:expr, $cost:expr, $object_id:expr) => {
        if is_object_shared_in_test($context, $object_id) {
            // TODO: Abort like this is impossible to debug.
            // Can we expose something more than just abort_code=0?
            return Ok(NativeResult::err(
                $cost,
                crate::natives::ECONSUME_SHARED_OBJECT,
            ));
        }
    };
}

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
            "emit_test_start_event",
            test_scenario::emit_test_start_event,
        ),
        (
            "TestScenario",
            "get_inventory",
            test_scenario::get_inventory,
        ),
        ("TestScenario", "num_events", test_scenario::num_events),
        (
            "TestScenario",
            "update_object",
            test_scenario::update_object,
        ),
        (
            "TestScenario",
            "transferred_object_ids",
            test_scenario::transferred_object_ids,
        ),
        (
            "Transfer",
            "delete_child_object_internal",
            transfer::delete_child_object_internal,
        ),
        ("Transfer", "transfer_internal", transfer::transfer_internal),
        ("Transfer", "freeze_object", transfer::freeze_object),
        ("Transfer", "share_object", transfer::share_object),
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

/// Given an object, return the most inner id bytes wrapped in `Value`.
// Object { id: VersionedID { id: UniqueID { id: ID { bytes: address } } } .. }
// Extract the first field of the struct 4 times to get the id bytes.
pub fn get_object_id_bytes_value(object: &Value) -> Value {
    get_nested_struct_field(object, &[0, 0, 0, 0])
}

/// Given an object, return the most inner id bytes.
/// Similar to get_object_id_bytes_value, but different return type.
pub fn get_object_id_bytes(object: &Value) -> AccountAddress {
    id_value_to_bytes(&get_object_id_bytes_value(object))
}

/// Given a VersionedID, return the most inner id bytes.
/// VersionedID { id: UniqueID { id: ID { bytes: address } } }
/// Extract the first field of the struct 3 times to get the bytes,
/// and convert it to AccountAddress.
pub fn get_versioned_id_bytes(id: &Value) -> AccountAddress {
    id_value_to_bytes(&get_nested_struct_field(id, &[0, 0, 0]))
}

/// Convert a wrapped AccountAddress Value into AccountAddress.
pub fn id_value_to_bytes(id: &Value) -> AccountAddress {
    id.copy_value()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
}

// Extract a field valye that's nested inside value `v`. The offset of each nesting
// is determined by `offsets`.
pub fn get_nested_struct_field(v: &Value, offsets: &[usize]) -> Value {
    let mut v = v.copy_value().unwrap();
    for offset in offsets {
        v = get_nth_struct_field(&v, *offset);
    }
    v
}

pub fn get_nth_struct_field(v: &Value, n: usize) -> Value {
    let mut itr = v
        .copy_value()
        .unwrap()
        .value_as::<Struct>()
        .unwrap()
        .unpack()
        .unwrap();
    itr.nth(n).unwrap()
}

fn is_object_shared_in_test(context: &mut NativeContext, object_id: AccountAddress) -> bool {
    if let Some(event) = context.events().iter().next() {
        let (_, event_type_byte, ..) = event;
        if *event_type_byte != TEST_START_EVENT {
            return false;
        }
        for (_, event_type_byte, _, _, val) in context.events() {
            match EventType::try_from_primitive(*event_type_byte as u8) {
                Err(_) => {
                    continue;
                }
                Ok(EventType::FreezeObject) | Ok(EventType::ShareObject) => {
                    let id = get_object_id_bytes(val);
                    if id == object_id {
                        return true;
                    }
                }
                Ok(_) => (),
            }
        }
    }
    false
}
