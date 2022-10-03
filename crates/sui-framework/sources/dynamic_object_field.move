// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Similar to `sui::dynamic_field`, this module allows for the access of dynamic fields. But
/// unlike, `sui::dynamic_field` the values bound to these dynamic fields _must_ be objects
/// themselves. This allows for the objects to still exist within in storage, which may be important
/// for external tools. The difference is otherwise not observable from within Move.
module sui::dynamic_object_field {

use std::option::{Self, Option};
use sui::dynamic_field::{
    hash_type_and_key,
    add_child_object,
    borrow_child_object,
    remove_child_object,
    has_child_object,
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

/// Internal object used for storing the field and the ID of the value
struct Field<Name: copy + drop + store> has key {
    /// Determined by the hash of the object ID, the field name value and it's type,
    /// i.e. hash(parent.id || name || Name)
    id: UID,
    /// The value for the name of this field
    name: Name,
    // TODO we need lamport timestamps to make this not an option
    /// The object bound to this field
    value: Option<ID>,
}

/// Adds a dynamic object field to the object `object: &mut UID` at field specified by `name: Name`.
/// Aborts with `EFieldAlreadyExists` if the object already has that field with that name.
public fun add<Name: copy + drop + store, Value: key + store>(
    // we use &mut UID in several spots for access control
    object: &mut UID,
    name: Name,
    value: Value,
) {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    if (!has_child_object(object_addr, hash)) {
        let field = Field {
            id: object::new_uid_from_hash(hash),
            name,
            value: option::none(),
        };
        add_child_object(object_addr, field)
    };
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    assert!(option::is_none(&field.value), EFieldAlreadyExists);
    option::fill(&mut field.value, object::id(&value));
    add_child_object(object::uid_to_address(&field.id), value);
}

/// Immutably borrows the `object`s dynamic object field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value object does not have the
/// specified type.
public fun borrow<Name: copy + drop + store, Value: key + store>(
    object: &UID,
    name: Name,
): &Value {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    let field_addr = object::uid_to_address(&field.id);
    let value_addr = object::id_to_address(option::borrow(&field.value));
    borrow_child_object<Value>(field_addr, value_addr)
}

/// Mutably borrows the `object`s dynamic object field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value object does not have the
/// specified type.
public fun borrow_mut<Name: copy + drop + store, Value: key + store>(
    object: &mut UID,
    name: Name,
): &mut Value {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    let field_addr = object::uid_to_address(&field.id);
    let value_addr = object::id_to_address(option::borrow(&field.value));
    borrow_child_object<Value>(field_addr, value_addr)
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
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    let field_addr = object::uid_to_address(&field.id);
    let value_id = option::extract(&mut field.value);
    let value_addr = object::id_to_address(&value_id);
    remove_child_object<Value>(field_addr, value_addr)
}

/// Returns true if and only if the `object` has a dynamic object field with the name specified by
/// `name: Name`.
public fun exists_<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): bool {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    if (!has_child_object(object_addr, hash)) return false;
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    option::is_some(&field.value)
}

/// Returns the ID of the object associated with the dynamic object field
/// Returns none otherwise
public fun id<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): Option<ID> {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    if (!has_child_object(object_addr, hash)) return option::none();
    let field = borrow_child_object<Field<Name>>(object_addr, hash);
    field.value
}

}
