// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Similar to `sui::dynamic_field`, this module allows for the access of dynamic fields. But
/// unlike, `sui::dynamic_field` the values bound to these dynamic fields _must_ be objects
/// themselves. This allows for the objects to still exist within in storage, which may be important
/// for external tools. The difference is otherwise not observable from within Move.
module sui::dynamic_object_field {

use std::option::{Self, Option};
use sui::dynamic_field::{
    Self as df,
    add_child_object,
    borrow_child_object,
    remove_child_object,
};
use sui::object::{Self, UID, ID};

/// The object already has a dynamic field with this name (with the value and type specified)
const EFieldAlreadyExists: u64 = 0;

/// Cannot load dynamic field.
/// The object does not have a dynamic field with this name (with the value and type specified)
const EFieldDoesNotExist: u64 = 1;

/// The object has a field with that name, but the value type does not match
const EFieldTypeMismatch: u64 = 2;

/// Failed to serialize the field's name
const EBCSSerializationFailure: u64 = 3;

// Internal object used for storing the field and the name associated with the value
// The separate type is necessary to prevent key collision with direct usage of dynamic_field
struct Wrapper<Name> has copy, drop, store {
    name: Name,
}

/// Adds a dynamic object field to the object `object: &mut UID` at field specified by `name: Name`.
/// Aborts with `EFieldAlreadyExists` if the object already has that field with that name.
public fun add<Name: copy + drop + store, Value: key + store>(
    // we use &mut UID in several spots for access control
    object: &mut UID,
    name: Name,
    value: Value,
) {
    let key = Wrapper { name };
    let id = object::id(&value);
    df::add(object, key, id);
    let (field_id, _) = df::field_ids<Wrapper<Name>>(object, key);
    add_child_object(object::id_to_address(&field_id), value);
}

/// Immutably borrows the `object`s dynamic object field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value object does not have the
/// specified type.
public fun borrow<Name: copy + drop + store, Value: key + store>(
    object: &UID,
    name: Name,
): &Value {
    let key = Wrapper { name };
    let (field_id, value_id) = df::field_ids<Wrapper<Name>>(object, key);
    borrow_child_object<Value>(object::id_to_address(&field_id), object::id_to_address(&value_id))
}

/// Mutably borrows the `object`s dynamic object field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value object does not have the
/// specified type.
public fun borrow_mut<Name: copy + drop + store, Value: key + store>(
    object: &mut UID,
    name: Name,
): &mut Value {
    let key = Wrapper { name };
    let (field_id, value_id) = df::field_ids<Wrapper<Name>>(object, key);
    borrow_child_object<Value>(object::id_to_address(&field_id), object::id_to_address(&value_id))
}

/// Removes the `object`s dynamic object field with the name specified by `name: Name` and returns
/// the bound object.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value object does not have the
/// specified type.
public fun remove<Name: copy + drop + store, Value: key + store>(
    object: &mut UID,
    name: Name,
): Value {
    let key = Wrapper { name };
    let (field_id, value_id) = df::field_ids<Wrapper<Name>>(object, key);
    let value = remove_child_object<Value>(
        object::id_to_address(&field_id),
        object::id_to_address(&value_id),
    );
    df::remove<Wrapper<Name>, ID>(object, key);
    value
}

/// Returns true if and only if the `object` has a dynamic object field with the name specified by
/// `name: Name`.
public fun exists_<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): bool {
    let key = Wrapper { name };
    df::exists_with_type<Wrapper<Name>, ID>(object, key)
}

/// Returns the ID of the object associated with the dynamic object field
/// Returns none otherwise
public fun id<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): Option<ID> {
    let key = Wrapper { name };
    if (!df::exists_with_type<Wrapper<Name>, ID>(object, key)) return option::none();
    let (_field_id, value_id) = df::field_ids<Wrapper<Name>>(object, key);
    option::some(value_id)
}

}
