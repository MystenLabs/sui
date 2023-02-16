// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// In addition to the fields declared in its type definition, a Sui object can have dynamic fields
/// that can be added after the object has been constructed. Unlike ordinary field names
/// (which are always statically declared identifiers) a dynamic field name can be any value with
/// the `copy`, `drop`, and `store` abilities, e.g. an integer, a boolean, or a string.
/// This gives Sui programmers the flexibility to extend objects on-the-fly, and it also serves as a
/// building block for core collection types
module sui::dynamic_field {

use sui::object::{Self, ID, UID};
use sui::prover;

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
    /// The value bound to this field
    value: Value,
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
    assert!(!has_child_object(object_addr, hash), EFieldAlreadyExists);
    let field = Field {
        id: object::new_uid_from_hash(hash),
        name,
        value,
    };
    add_child_object(object_addr, field)
}

spec add {
    pragma opaque;

    let addr  = object.id.bytes;

    // the function aborts when the key already exists
    aborts_if [abstract] spec_uid_has_field(object, name);

    // this function, upon completion, will update the shards
    modifies [abstract] global<NameShard<Name>>(addr);
    modifies [abstract] global<PairShard<Name, Value>>(addr);

    // function preserves object ID
    ensures [abstract] object.id == old(object.id);

    // update to the name shard
    ensures [abstract] exists<NameShard<Name>>(addr);
    ensures [abstract] (!old(exists<NameShard<Name>>(addr)))
        ==> global<NameShard<Name>>(addr).names == vec(name);
    ensures [abstract] old(exists<NameShard<Name>>(addr))
        ==> global<NameShard<Name>>(addr).names == concat(
                old(global<NameShard<Name>>(addr).names),
                vec(name)
            );

    // update to the kv-pair shard
    ensures [abstract] exists<PairShard<Name, Value>>(addr);
    ensures [abstract] (!old(exists<PairShard<Name, Value>>(addr)))
        ==> global<PairShard<Name, Value>>(addr).entries == prover::map_set(
                prover::map_new(), name, value
            );
    ensures [abstract] old(exists<PairShard<Name, Value>>(addr))
        ==> global<PairShard<Name, Value>>(addr).entries == prover::map_set(
                old(global<PairShard<Name, Value>>(addr).entries),
                name, value
            );
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
    let field = borrow_child_object<Field<Name, Value>>(object, hash);
    &field.value
}

spec borrow {
    pragma opaque;
    aborts_if [abstract] !spec_uid_has_field(object, name);
    aborts_if [abstract] !spec_uid_has_field_with_type<Name, Value>(object, name);
    ensures result == spec_uid_get_value(object, name);
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
    let field = borrow_child_object_mut<Field<Name, Value>>(object, hash);
    &mut field.value
}

spec borrow_mut {
    pragma intrinsic;
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
    let Field { id, name: _, value } = remove_child_object<Field<Name, Value>>(object_addr, hash);
    object::delete(id);
    value
}

spec remove {
    pragma opaque;

    let addr  = object.id.bytes;

    // the function aborts when the key does not exist
    aborts_if [abstract] !spec_uid_has_field(object, name);

    // the function aborts when the key exists but value type does not match
    aborts_if [abstract] !spec_uid_has_field_with_type<Name, Value>(object, name);

    // this function, upon completion, will update the shards
    modifies [abstract] global<NameShard<Name>>(addr);
    modifies [abstract] global<PairShard<Name, Value>>(addr);

    // function preserves object ID
    ensures [abstract] object.id == old(object.id);

    // update to the name shard
    ensures [abstract] exists<NameShard<Name>>(addr);
    ensures [abstract] global<NameShard<Name>>(addr).names == sui::prover_vec::remove(
        old(global<NameShard<Name>>(addr).names),
        index_of(old(global<NameShard<Name>>(addr).names), name)
    );

    // update to the kv-pair shard
    ensures [abstract] exists<PairShard<Name, Value>>(addr);
    ensures [abstract] global<PairShard<Name, Value>>(addr).entries == prover::map_del(
        old(global<PairShard<Name, Value>>(addr).entries), name
    );
}


/// Returns true if and only if the `object` has a dynamic field with the name specified by
/// `name: Name` but without specifying the `Value` type
public fun exists_<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): bool {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    has_child_object(object_addr, hash)
}

spec exists_ {
    pragma opaque;
    aborts_if [abstract] false;
    ensures result == spec_uid_has_field(object, name);
}

/// Returns true if and only if the `object` has a dynamic field with the name specified by
/// `name: Name` with an assigned value of type `Value`.
public fun exists_with_type<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): bool {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    has_child_object_with_ty<Field<Name, Value>>(object_addr, hash)
}

spec exists_with_type {
    pragma opaque;
    aborts_if [abstract] false;
    ensures result == spec_uid_has_field_with_type<Name, Value>(object, name);
}

public(friend) fun field_info<Name: copy + drop + store>(
    object: &UID,
    name: Name,
): (&UID, address) {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let Field { id, name: _, value } = borrow_child_object<Field<Name, ID>>(object, hash);
    (id, object::id_to_address(value))
}

spec field_info {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) fun field_info_mut<Name: copy + drop + store>(
    object: &mut UID,
    name: Name,
): (&mut UID, address) {
    let object_addr = object::uid_to_address(object);
    let hash = hash_type_and_key(object_addr, name);
    let Field { id, name: _, value } = borrow_child_object_mut<Field<Name, ID>>(object, hash);
    (id, object::id_to_address(value))
}

spec field_info_mut {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) native fun hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address;

spec hash_type_and_key {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) native fun add_child_object<Child: key>(parent: address, child: Child);

spec add_child_object {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

/// throws `EFieldDoesNotExist` if a child does not exist with that ID
/// or throws `EFieldTypeMismatch` if the type does not match
/// we need two versions to return a reference or a mutable reference
public(friend) native fun borrow_child_object<Child: key>(object: &UID, id: address): &Child;

spec borrow_child_object {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) native fun borrow_child_object_mut<Child: key>(object: &mut UID, id: address): &mut Child;

spec borrow_child_object_mut {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

/// throws `EFieldDoesNotExist` if a child does not exist with that ID
/// or throws `EFieldTypeMismatch` if the type does not match
public(friend) native fun remove_child_object<Child: key>(parent: address, id: address): Child;

spec remove_child_object {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) native fun has_child_object(parent: address, id: address): bool;

spec has_child_object {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

public(friend) native fun has_child_object_with_ty<Child: key>(parent: address, id: address): bool;

spec has_child_object_with_ty {
    pragma opaque;
    // TODO: stub to be replaced by actual abort conditions if any
    aborts_if [abstract] true;
    // TODO: specify actual function behavior
}

// === Dynamic fields sharded by key-value types ===

#[verify_only]
/// A slice of the dynamic fields associated with an object sharded by K type.
struct NameShard<Name: copy + drop + store> has key {
    names: vector<Name>,
}
spec NameShard {
    // this makes the vector a set
    invariant forall i in 0..len(names), j in 0..len(names):
        names[i] == names[j] ==> i == j;
}

#[verify_only]
/// A slice of the dynamic fields associated with an object sharded by K-V type.
struct PairShard<phantom Name: copy + drop + store, phantom Value: store> has key {
    entries: prover::Map<Name, Value>,
}

spec module {
    invariant<Name, Value> forall addr: address:
        exists<PairShard<Name, Value>>(addr) ==> exists<NameShard<Name>>(addr);

    invariant<Name, Value> forall addr: address
        where exists<NameShard<Name>>(addr) && exists<PairShard<Name, Value>>(addr):
            forall k: Name where prover::map_contains(global<PairShard<Name, Value>>(addr).entries, k):
                contains(global<NameShard<Name>>(addr).names, k);

    invariant<Name, Value> forall addr: address
        where exists<NameShard<Name>>(addr) && exists<PairShard<Name, Value>>(addr):
            forall k: Name where !contains(global<NameShard<Name>>(addr).names, k):
                !prover::map_contains(global<PairShard<Name, Value>>(addr).entries, k);
}

// === Utility spec functions ===

/// Checks if a given object has field with a given name.
spec fun spec_has_field<T: key, Name: copy + drop + store>(obj: T, name: Name): bool {
    let uid = object::borrow_uid(obj);
    spec_uid_has_field<Name>(uid, name)
}

spec fun spec_uid_has_field<Name: copy + drop + store>(uid: object::UID, name: Name): bool {
    let addr = object::uid_to_address(uid);
    exists<NameShard<Name>>(addr) &&
        contains(global<NameShard<Name>>(addr).names, name)
}

/// Checks if a given object has field with a given name and type.
spec fun spec_has_field_with_type<T: key, Name: copy + drop + store, Value: store>(obj: T, name: Name): bool {
    let uid = object::borrow_uid(obj);
    spec_uid_has_field_with_type<Name, Value>(uid, name)
}

spec fun spec_uid_has_field_with_type<Name: copy + drop + store, Value: store>(uid: object::UID, name: Name): bool {
    let addr = object::uid_to_address(uid);
    exists<PairShard<Name, Value>>(addr) &&
        prover::map_contains(global<PairShard<Name, Value>>(addr).entries, name)
}

/// Returns number of K-type fields of a given object.
spec fun spec_num_fields<T: key, Name: copy + drop + store>(obj: T): num {
    let uid = object::borrow_uid(obj);
    spec_uid_num_fields<Name>(uid)
}

spec fun spec_uid_num_fields<Name: copy + drop + store>(uid: object::UID): num {
    let addr = object::uid_to_address(uid);
    if (!exists<NameShard<Name>>(addr)) {
        0
    } else {
        len(global<NameShard<Name>>(addr).names)
    }
}

/// Returns number of K-type fields of a given object with a given value type.
spec fun spec_num_fields_with_type<T: key, Name: copy + drop + store, Value: store>(obj: T): num {
    let uid = object::borrow_uid(obj);
    spec_uid_num_fields_with_type<Name, Value>(uid)
}

spec fun spec_uid_num_fields_with_type<Name: copy + drop + store, Value: store>(uid: object::UID): num {
    let addr = object::uid_to_address(uid);
    if (!exists<PairShard<Name, Value>>(addr)) {
        0
    } else {
        prover::map_len(global<PairShard<Name, Value>>(addr).entries)
    }
}

/// Returns number of K-type fields of a given object with a given value type.
spec fun spec_get_value<T: key, Name: copy + drop + store, Value: store>(obj: T, name: Name): Value {
    let uid = object::borrow_uid(obj);
    spec_uid_get_value<Name, Value>(uid, name)
}

spec fun spec_uid_get_value<Name: copy + drop + store, Value: store>(uid: object::UID, name: Name): Value {
    let addr = object::uid_to_address(uid);
    prover::map_get(global<PairShard<Name, Value>>(addr).entries, name)
}

}
