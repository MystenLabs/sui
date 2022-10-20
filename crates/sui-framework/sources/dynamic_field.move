// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// In addition to the fields declared in its type definition, a Sui object can have dynamic fields
/// that can be added after the object has been constructed. Unlike ordinary field names
/// (which are always statically declared identifiers) a dynamic field name can be any value with
/// the `copy`, `drop`, and `store` abilities, e.g. an integer, a boolean, or a string.
/// This gives Sui programmers the flexibility to extend objects on-the-fly, and it also serves as a
/// building block for core collection types
module sui::dynamic_field {

use std::option::{Self, Option};
use sui::object::{Self, ID, UID};

friend sui::dynamic_object_field;

/// The object already has a dynamic field with this name (with the value and type specified)
const EFieldAlreadyExists: u64 = 0;

/// Cannot load dynamic field.
/// The object does not have a dynamic field with this name (with the value and type specified)
const EFieldDoesNotExist: u64 = 1;

/// The object has a field with that name, but the value type does not match
const EFieldTypeMismatch: u64 = 2;

/// Failed to serialize the field's name
const EBCSSerializationFailure: u64 = 3;

/// Internal object used for storing the field and value
struct Field<Name: copy + drop + store, Value: store> has key {
    /// Determined by the hash of the object ID, the field name value and it's type,
    /// i.e. hash(parent.id || name || Name)
    id: UID,
    /// The value for the name of this field
    name: Name,
    // TODO we need lamport timestamps to make this not an option
    /// The value bound to this field
    value: Option<Value>,
}

/// Adds a dynamic field to the object `object: &mut UID` at field specified by `name: Name`.
/// Aborts with `EFieldAlreadyExists` if the object already has that field with that name.
public fun add<Name: copy + drop + store, Value: store>(
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
            value: option::none<Value>(),
        };
        add_child_object(object_addr, field)
    };
    // TODO remove once we have lamport timestamps
    assert!(has_child_object_with_ty<Field<Name, Value>>(object_addr, hash), EFieldAlreadyExists);
    let field = borrow_child_object<Field<Name, Value>>(object_addr, hash);
    assert!(option::is_none(&field.value), EFieldAlreadyExists);
    option::fill(&mut field.value, value);
}

/// Immutably borrows the `object`s dynamic field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value does not have the specified
/// type.
public fun borrow<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): &Value {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name, Value>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    option::borrow(&field.value)
}

/// Mutably borrows the `object`s dynamic field with the name specified by `name: Name`.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value does not have the specified
/// type.
public fun borrow_mut<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): &mut Value {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name, Value>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    option::borrow_mut(&mut field.value)
}

/// Removes the `object`s dynamic field with the name specified by `name: Name` and returns the
/// bound value.
/// Aborts with `EFieldDoesNotExist` if the object does not have a field with that name.
/// Aborts with `EFieldTypeMismatch` if the field exists, but the value does not have the specified
/// type.
public fun remove<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): Value {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name, Value>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    option::extract(&mut field.value)
}

// TODO implement exists (without the Value type) once we have lamport timestamps
/// Returns true if and only if the `object` has a dynamic field with the name specified by
/// `name: Name` with an assigned value of type `Value`.
public fun exists_with_type<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): bool {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    if (!has_child_object_with_ty<Field<Name, Value>>(object_addr, hash)) return false;
    let field = borrow_child_object<Field<Name, Value>>(object_addr, hash);
    option::is_some(&field.value)
}

public(friend) fun field_ids<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): (address, address) {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let field = borrow_child_object<Field<Name, ID>>(object_addr, hash);
    assert!(option::is_some(&field.value), EFieldDoesNotExist);
    (object::uid_to_address(&field.id), object::id_to_address(&option::destroy_some(field.value)))
}

public(friend) native fun hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address;

public(friend) native fun add_child_object<Child: key>(parent: address, child: Child);

/// throws `EFieldDoesNotExist` if a child does not exist with that ID
/// or throws `EFieldTypeMismatch` if the type does not match
public(friend) native fun borrow_child_object<Child: key>(parent: address, id: address): &mut Child;

/// throws `EFieldDoesNotExist` if a child does not exist with that ID
/// or throws `EFieldTypeMismatch` if the type does not match
public(friend) native fun remove_child_object<Child: key>(parent: address, id: address): Child;

public(friend) native fun has_child_object(parent: address, id: address): bool;

public(friend) native fun has_child_object_with_ty<Child: key>(parent: address, id: address): bool;

}
