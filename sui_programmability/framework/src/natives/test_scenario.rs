// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
use core::panic;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, value::MoveTypeLayout};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::{Struct, Value},
};
use num_enum::TryFromPrimitive;
use smallvec::smallvec;
use std::collections::{BTreeMap, VecDeque};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Owner,
};

type Event = (Vec<u8>, u64, Type, MoveTypeLayout, Value);

const WRAPPED_OBJECT_EVENT: u64 = 255;

#[derive(Debug)]
struct OwnedObj {
    value: Value,
    type_: Type,
    owner: Owner,
}

/// Set of all live objects in the current test scenario
// TODO: add a native function that prints the inventory for debugging purposes
// This will require extending NativeContext with a function to map `Type` (which is just an index
// into the module's StructHandle table for structs) to something human-readable like `TypeTag`.
// TODO: add a native function that prints the log of transfers, deletes, wraps for debugging purposes
type Inventory = BTreeMap<ObjectID, OwnedObj>;

fn get_id(versioned_id: &Value) -> AccountAddress {
    // All of the following unwraps are safe by construction because a properly verified
    // bytecode ensures that the parameter must be of type UniqueID with the correct fields:
    // ```
    // VersionedID { id: UniqueID { id: ID { bytes: address } } .. }
    // ```
    versioned_id
        .copy_value()
        .unwrap()
        .value_as::<Struct>()
        .unwrap() // Convert to VersionedID
        .unpack()
        .unwrap()
        .next() // Get id field of VersionedID
        .unwrap()
        .value_as::<Struct>() // Convert to UniqueID
        .unwrap()
        .unpack()
        .unwrap()
        .next()
        .unwrap() // Get id field of UniqueID
        .value_as::<Struct>() // Convert to ID
        .unwrap()
        .unpack()
        .unwrap()
        .next()
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
}

/// Process the event log to determine the global set of live objects
fn get_global_inventory(events: &[Event]) -> Inventory {
    let mut inventory = Inventory::new();
    for (recipient, event_type_byte, type_, layout, val) in events {
        if *event_type_byte == WRAPPED_OBJECT_EVENT {
            // special, TestScenario-only event for object wrapping. treat the same as DeleteObjectID for inventory purposes--a wrapped object is not available for use
            let obj_id = ObjectID::try_from(recipient.as_slice()).unwrap();
            assert!(inventory.remove(&obj_id).is_some());
            continue;
        }
        let event_type = EventType::try_from_primitive(*event_type_byte as u8)
            .expect("This will always succeed for a well-structured event log");
        match event_type {
            EventType::TransferToAddress
            | EventType::TransferToObject
            | EventType::FreezeObject => {
                let obj_bytes = val
                    .simple_serialize(layout)
                    .expect("This will always succeed for a well-structured event log");
                let obj_id = ObjectID::try_from(&obj_bytes[0..ObjectID::LENGTH])
                    .expect("This will always succeed on an object from a system transfer event");
                let owner = match event_type {
                    EventType::FreezeObject => Owner::SharedImmutable,
                    EventType::TransferToAddress => {
                        Owner::AddressOwner(SuiAddress::try_from(recipient.clone()).unwrap())
                    }
                    EventType::TransferToObject => {
                        Owner::ObjectOwner(SuiAddress::try_from(recipient.clone()).unwrap())
                    }
                    _ => panic!("Unrecognized event_type"),
                };
                // note; may overwrite older values of the object, which is intended
                inventory.insert(
                    obj_id,
                    OwnedObj {
                        value: Value::copy_value(val).unwrap(),
                        type_: type_.clone(),
                        owner,
                    },
                );
            }
            EventType::DeleteObjectID => {
                let obj_id = get_id(val);
                // note: obj_id may or may not be present in `inventory`--a useer can create an ID and delete it without associating it with a transferred object
                inventory.remove(&obj_id.into());
            }
            EventType::User => (),
        }
    }
    inventory
}

/// Get the objects of type `type_` that can be spent by `addr`
fn get_inventory_for(
    addr: &AccountAddress,
    type_: &Type,
    tx_end_index: usize,
    events: &[Event],
) -> Vec<Value> {
    let inventory = get_global_inventory(&events[..tx_end_index]);
    let sui_addr = SuiAddress::try_from(addr.to_vec()).unwrap();
    inventory
        .into_iter()
        .filter_map(|(_, obj)| {
            // TODO: We should also be able to include objects indirectly owned by the
            // requested address through owning other objects.
            // https://github.com/MystenLabs/sui/issues/673
            if (obj.owner == Owner::AddressOwner(sui_addr) || obj.owner.is_shared())
                && &obj.type_ == type_
            {
                Some(obj.value)
            } else {
                None
            }
        })
        .collect()
}

/// Return the ID's of objects deleted since a given `tx_begin_idx`
pub fn deleted_object_ids(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert_eq!(args.len(), 1);

    let tx_begin_idx = pop_arg!(args, u64) as usize;

    let deleted_ids: Vec<Value> = context
        .events()
        .iter()
        .skip(tx_begin_idx)
        .filter_map(|(_, event_type_byte, _, _, val)| {
            if *event_type_byte == EventType::DeleteObjectID as u64 {
                // TODO: for some reason, this creates internal type errors in the VM. Create a fresh value with the object ID instead
                //Some(Value::copy_value(val).unwrap())
                let id_bytes = get_id(val).to_vec();
                Some(Value::vector_u8(id_bytes))
            } else {
                None
            }
        })
        .collect();

    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_for_testing_only(deleted_ids)],
    ))
}

/// Return the ID's of objects transferred since a given `tx_begin_idx`
// Note: if an object was transferred, but subsequently deleted, it will not appear in the return values
pub fn transferred_object_ids(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert_eq!(args.len(), 1);

    let tx_begin_idx = pop_arg!(args, u64) as usize;

    let transferred_ids: Vec<Value> =
        get_global_inventory(&context.events().as_slice()[tx_begin_idx..])
            .into_keys()
            .map(|obj_id| Value::vector_u8(obj_id.to_vec()))
            .collect();

    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_for_testing_only(transferred_ids)],
    ))
}

/// Emit a special event that is only meaningful to `TestScenario`: object wrapping
pub fn emit_wrapped_object_event(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert_eq!(args.len(), 1);

    let wrapped_id = pop_arg!(args, Vec<u8>);
    // pick dummy type/value--these won't be inspected by the consumer of the event, only wrapped_id matters
    let dummy_type = Type::Bool;
    let dummy_value = Value::bool(true);
    context.save_event(wrapped_id, WRAPPED_OBJECT_EVENT, dummy_type, dummy_value)?;
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(cost, smallvec![]))
}

/// Return the number of events emitted, including both user-defined events and system events
pub fn num_events(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    // Gas amount doesn't matter as this is test only.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);

    let num_events = context.events().len();
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::u64(num_events as u64)],
    ))
}

/// Return all the values of type `T` in the inventory of `owner_address`
pub fn get_inventory(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 2);

    let tx_end_index = pop_arg!(args, u64) as usize;
    let owner_address = pop_arg!(args, AccountAddress);

    let inventory = get_inventory_for(&owner_address, &ty_args[0], tx_end_index, context.events());
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_for_testing_only(inventory)],
    ))
}

/// Delete the given object
pub fn delete_object_for_testing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 1);

    // Gas amount doesn't matter as this is test only.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(cost, smallvec![]))
}
