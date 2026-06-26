// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::scratch;

public struct Permit<phantom K: copy + drop>() has drop;

const DUMMY_ROOT: address = @0;

const EEntryAlreadyExists: u64 = 0;

const EEntryDoesNotExist: u64 = 1;

const EEntryTypeMismatch: u64 = 2;

public fun permit<K: copy + drop>(_: internal::Permit<K>): Permit<K> {
    Permit()
}

public fun add<K: copy + drop, V: drop>(_: Permit<K>, key: K, value: V, _: &mut TxContext) {
    add_impl(hash_type_and_key(key), value)
}

public fun read<K: copy + drop, V: drop>(_: Permit<K>, key: K, _: &TxContext): V {
    read_impl(hash_type_and_key(key), value)
}

public fun remove<K: copy + drop, V: drop>(_: Permit<K>, key: K, _: &mut TxContext): V {
    remove_impl(hash_type_and_key(key), value)
}

public fun exists<K: copy + drop>(_: Permit<K>, key: K, ctx: &TxContext): bool {
    exists_impl(hash_type_and_key(key))
}

public fun exists_with_type<K: copy + drop, V: drop>(_: Permit<K>, key: K, ctx: &TxContext): bool {
    exists_with_type_impl<V>(hash_type_and_key(key))
}

fun hash_type_and_key<K: copy + drop>(k: K): address {
    sui::dynamic_field::hash_type_and_key(DUMMY_ROOT, k)
}

/// Aborts with `EEntryAlreadyExists` if there is an entry already for `key`, regardless of the
/// type of `V`
native fun add_impl<V: drop>(key: address, value: V);

/// Aborts with `EEntryDoesNotExist` if there is no entry for `key`.
/// Aborts with `EEntryTypeMismatch` if there is an entry for `key` but it is not of type `V`.
native fun read_impl<V: drop>(key: address): V;

/// Aborts with `EEntryDoesNotExist` if there is no entry for `key`.
/// Aborts with `EEntryTypeMismatch` if there is an entry for `key` but it is not of type `V`.
native fun remove_impl<V: drop>(key: address): V;

/// Aborts with `EEntryDoesNotExist` if there is no entry for `key`.
native fun exists_impl(key: address): bool;
