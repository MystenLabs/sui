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
    values::{StructRef, Value, VectorRef},
};
use num_enum::TryFromPrimitive;
use smallvec::smallvec;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Owner,
};

use super::{get_nested_struct_field, get_object_id};

type Event = (Vec<u8>, u64, Type, MoveTypeLayout, Value);

const WRAPPED_OBJECT_EVENT: u64 = 255;
const UPDATE_OBJECT_EVENT: u64 = 254;

/// When transfer an object to a parent object, the parent object
/// is not found in the inventory.
const EPARENT_OBJECT_NOT_FOUND: u64 = 100;

#[derive(Debug)]
struct OwnedObj {
    value: Value,
    type_: Type,
    /// Owner is the direct owner of the object.
    owner: Owner,
    /// Signer is the ultimate owner of the object potentially through
    /// chains of object ownership.
    /// e.g. If account A ownd object O1, O1 owns O2. Then
    /// O2's owner is O1, and signer is A.
    /// signer will always be set eventually, but it needs to be optional first
    /// since we may not know its signer initially.
    signer: Option<Owner>,
}

/// Set of all live objects in the current test scenario
// TODO: add a native function that prints the inventory for debugging purposes
// This will require extending NativeContext with a function to map `Type` (which is just an index
// into the module's StructHandle table for structs) to something human-readable like `TypeTag`.
// TODO: add a native function that prints the log of transfers, deletes, wraps for debugging purposes
type Inventory = BTreeMap<ObjectID, OwnedObj>;

/// Return the object ID involved in an event.
/// This depends on the value format for each event type.
fn get_object_id_from_event(event_type_byte: u64, val: &Value) -> Option<ObjectID> {
    let val = val.copy_value().unwrap();
    let address = if event_type_byte == WRAPPED_OBJECT_EVENT {
        val
    } else if event_type_byte == UPDATE_OBJECT_EVENT {
        get_object_id(val).unwrap()
    } else {
        let event_type = EventType::try_from_primitive(event_type_byte as u8).unwrap();
        match event_type {
            EventType::DeleteChildObject => val,
            EventType::DeleteObjectID => get_nested_struct_field(val, &[0, 0, 0]).unwrap(),
            EventType::User => {
                return None;
            }
            _ => get_object_id(val.copy_value().unwrap()).unwrap(),
        }
    };
    Some(ObjectID::try_from(address.value_as::<AccountAddress>().unwrap().as_slice()).unwrap())
}

fn account_to_sui_address(address: AccountAddress) -> SuiAddress {
    SuiAddress::try_from(address.as_slice()).unwrap()
}

/// Process the event log to determine the global set of live objects
/// Returns the abort_code if an error is encountered.
fn get_global_inventory(events: &[Event]) -> Result<Inventory, u64> {
    let mut inventory = Inventory::new();
    // Since we allow transfer object to ID, it's possible that when we transfer
    // an object to a parenet object, the parent object does not yet exist in the event log.
    // And without the parent object we cannot know the ultimate signer.
    // To handle this, for child objects whose parent is not yet known, we add them
    // to the unresolved_signer_parents map, which maps from parent object ID
    // to the list of child objects it has. Whenever a new object is seen, we check the map
    // and resolve if the object is an unresolved parent.
    let mut unresolved_signer_parents: BTreeMap<ObjectID, BTreeSet<ObjectID>> = BTreeMap::new();
    for (recipient, event_type_byte, type_, _layout, val) in events {
        let obj_id = if let Some(obj_id) = get_object_id_from_event(*event_type_byte, val) {
            obj_id
        } else {
            continue;
        };

        if *event_type_byte == WRAPPED_OBJECT_EVENT {
            // special, TestScenario-only event for object wrapping. treat the same as DeleteObjectID for inventory purposes--a wrapped object is not available for use
            assert!(inventory.remove(&obj_id).is_some());
            continue;
        }
        if *event_type_byte == UPDATE_OBJECT_EVENT {
            if let Some(cur) = inventory.get_mut(&obj_id) {
                let new_value = val.copy_value().unwrap();
                // Update the object content since it may have been mutated.
                cur.value = new_value;
            }
            continue;
        }
        let event_type = EventType::try_from_primitive(*event_type_byte as u8)
            .expect("This will always succeed for a well-structured event log");
        match event_type {
            EventType::TransferToAddress
            | EventType::TransferToObject
            | EventType::FreezeObject
            | EventType::ShareObject => {
                let owner = get_new_owner(&event_type, recipient.clone());
                let signer = if event_type == EventType::TransferToObject {
                    let parent_id = ObjectID::try_from(recipient.as_slice()).unwrap();
                    if let Some(parent_obj) = inventory.get(&parent_id) {
                        parent_obj.signer
                    } else {
                        unresolved_signer_parents
                            .entry(parent_id)
                            .or_default()
                            .insert(obj_id);
                        None
                    }
                } else {
                    Some(owner)
                };
                if signer.is_some() {
                    if let Some(children) = unresolved_signer_parents.remove(&obj_id) {
                        for child in children {
                            inventory.get_mut(&child).unwrap().signer = signer;
                        }
                    }
                }
                // note; may overwrite older values of the object, which is intended
                inventory.insert(
                    obj_id,
                    OwnedObj {
                        value: Value::copy_value(val).unwrap(),
                        type_: type_.clone(),
                        owner,
                        signer,
                    },
                );
            }
            EventType::DeleteObjectID | EventType::DeleteChildObject => {
                // note: obj_id may or may not be present in `inventory`--a useer can create an ID and delete it without associating it with a transferred object
                inventory.remove(&obj_id);
            }
            EventType::User => (),
        }
    }
    if unresolved_signer_parents.is_empty() {
        Ok(inventory)
    } else {
        Err(EPARENT_OBJECT_NOT_FOUND)
    }
}

/// Return the new owner of the object after the transfer event.
fn get_new_owner(event_type: &EventType, recipient: Vec<u8>) -> Owner {
    match event_type {
        EventType::FreezeObject => Owner::Immutable,
        EventType::ShareObject => Owner::Shared,
        EventType::TransferToAddress => {
            Owner::AddressOwner(SuiAddress::try_from(recipient).unwrap())
        }
        EventType::TransferToObject => Owner::ObjectOwner(SuiAddress::try_from(recipient).unwrap()),
        _ => panic!("Unrecognized event_type"),
    }
}

/// Get the objects of type `type_` that can be spent by `addr`
/// Returns the abort_code if an error is encountered.
fn get_inventory_for(
    signer: Owner,
    parent_object: Option<AccountAddress>,
    type_: &Type,
    tx_end_index: usize,
    events: &[Event],
) -> Result<Vec<Value>, u64> {
    let inventory = get_global_inventory(&events[..tx_end_index])?;
    Ok(inventory
        .into_iter()
        .filter(|(_, obj)| {
            &obj.type_ == type_
                && if let Some(parent) = parent_object {
                    let obj_signer = obj.signer.unwrap();
                    obj.owner
                        == Owner::ObjectOwner(SuiAddress::try_from(parent.as_slice()).unwrap())
                        && (!obj_signer.is_owned() || obj_signer == signer)
                } else {
                    obj.owner == signer
                }
        })
        .map(|(_, obj)| obj.value)
        .collect())
}

pub fn emit_wrapped_object_events(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 2);

    let id_type = ty_args.pop().unwrap();
    let removed = pop_arg!(args, VectorRef);
    let tx_begin_idx = pop_arg!(args, u64) as usize;

    let mut removed_ids: BTreeSet<ObjectID> = BTreeSet::new();
    for i in 0..removed.len(&id_type)?.value_as::<u64>()? {
        let id = removed.borrow_elem(i as usize, &id_type)?;
        let id_bytes = get_nested_struct_field(id.value_as::<StructRef>()?.read_ref()?, &[0])?;
        removed_ids.insert(id_bytes.value_as::<AccountAddress>()?.into());
    }

    let processed_ids: BTreeSet<_> = context.events()[tx_begin_idx..]
        .iter()
        .filter_map(|(_, event_type_byte, _, _, val)| {
            get_object_id_from_event(*event_type_byte, val)
        })
        .collect();
    // Any object that was removed (and not returned) during the current transaction,
    // but did not appear in any of the events, must be wrapped.
    for id in removed_ids.difference(&processed_ids) {
        context.save_event(
            vec![],
            WRAPPED_OBJECT_EVENT,
            Type::Address,
            Value::address((*id).into()),
        )?;
    }

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
pub fn get_account_owned_inventory(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 2);

    let tx_end_index = pop_arg!(args, u64) as usize;
    let owner_address = pop_arg!(args, AccountAddress);

    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    match get_inventory_for(
        Owner::AddressOwner(account_to_sui_address(owner_address)),
        None,
        &ty_args[0],
        tx_end_index,
        context.events(),
    ) {
        Ok(inventory) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_for_testing_only(inventory)],
        )),
        Err(abort_code) => Ok(NativeResult::err(cost, abort_code)),
    }
}

pub fn get_unowned_inventory(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 2);

    let tx_end_index = pop_arg!(args, u64) as usize;
    let immutable = pop_arg!(args, bool);
    let owner = if immutable {
        Owner::Immutable
    } else {
        Owner::Shared
    };

    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    match get_inventory_for(owner, None, &ty_args[0], tx_end_index, context.events()) {
        Ok(inventory) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_for_testing_only(inventory)],
        )),
        Err(abort_code) => Ok(NativeResult::err(cost, abort_code)),
    }
}

pub fn get_object_owned_inventory(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 3);

    let tx_end_index = pop_arg!(args, u64) as usize;
    let parent_object = pop_arg!(args, AccountAddress);
    let signer_address = pop_arg!(args, AccountAddress);

    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    match get_inventory_for(
        Owner::AddressOwner(account_to_sui_address(signer_address)),
        Some(parent_object),
        &ty_args[0],
        tx_end_index,
        context.events(),
    ) {
        Ok(inventory) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_for_testing_only(inventory)],
        )),
        Err(abort_code) => Ok(NativeResult::err(cost, abort_code)),
    }
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

pub fn update_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert_eq!(ty_args.len(), 1);
    debug_assert_eq!(args.len(), 1);

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();

    // Gas amount doesn't matter as this is test only.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    context.save_event(vec![], UPDATE_OBJECT_EVENT, ty, obj)?;
    // Run through the events to make sure the object we returned didn't violate any rules.
    match get_global_inventory(context.events()) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![])),
        Err(abort_code) => Ok(NativeResult::err(cost, abort_code)),
    }
}
