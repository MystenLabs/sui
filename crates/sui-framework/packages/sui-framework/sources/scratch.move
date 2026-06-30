// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// `sui::scratch` is an ephemeral, per-transaction key-value store. Unlike `sui::dynamic_field`,
/// scratch entries are not attached to any object, and are instead dropped at the end of the
/// transaction.
///
/// Each entry is identified by the pair of its key type and key value, hashed together in the same
/// way as a dynamic field name (see `sui::dynamic_field::hash_type_and_key`).
///
/// All access (mutable and immutable) is controlled through the module that defines the key type
/// `K`. The functions are gated by a `Permit<K>`, which can be granted via an
/// `internal::Permit<K>`.
module sui::scratch;

/// A `Permit<K>` gates access to all entries keyed by values of type `K`.
/// It is issued from an `internal::Permit<K>`, allowing the module that defines `K` to control
/// all access to scratch entries.
public struct Permit<phantom K: copy + drop>() has drop;

/// Stand-in parent address used when hashing scratch keys.
const DUMMY_ROOT: address = @0;

/// The scratch store already has an entry for this key.
const EEntryAlreadyExists: u64 = 0;

/// The scratch store does not have an entry for this key.
const EEntryDoesNotExist: u64 = 1;

/// The scratch store has an entry for this key, but the value type does not match.
const EEntryTypeMismatch: u64 = 2;

/// Issues a `Permit<K>` from the privileged `internal::Permit<K>`, granting access to the
/// scratch entries keyed by values of type `K`.
public fun permit<K: copy + drop>(_: internal::Permit<K>): Permit<K> {
    Permit()
}

/// Adds the `key`-`value` pair to the scratch store. Requires a `Permit<K>` for the key type.
/// Aborts with `EEntryAlreadyExists` if there is already an entry for `key`, regardless of its
/// value type.
public fun add<K: copy + drop, V: drop>(_: Permit<K>, key: K, value: V, _: &mut TxContext) {
    add_impl(hash_type_and_key(key), value)
}

/// Returns a copy of the value bound to `key`. Requires a `Permit<K>` for the key type.
/// Aborts with `EEntryDoesNotExist` if there is no entry for `key`.
/// Aborts with `EEntryTypeMismatch` if the entry exists, but its value is not of type `V`.
public fun read<K: copy + drop, V: drop>(_: Permit<K>, key: K, _: &TxContext): V {
    read_impl(hash_type_and_key(key))
}

/// Removes the entry bound to `key` and returns its value. Requires a `Permit<K>` for the key type.
/// Aborts with `EEntryDoesNotExist` if there is no entry for `key`.
/// Aborts with `EEntryTypeMismatch` if the entry exists, but its value is not of type `V`.
public fun remove<K: copy + drop, V: drop>(_: Permit<K>, key: K, _: &mut TxContext): V {
    remove_impl(hash_type_and_key(key))
}

/// Returns true if and only if the scratch store has an entry for `key`, without regard to the
/// value type. Requires a `Permit<K>` for the key type.
public fun exists<K: copy + drop>(_: Permit<K>, key: K, _: &TxContext): bool {
    exists_impl(hash_type_and_key(key))
}

/// Returns true if and only if the scratch store has an entry for `key` whose value is of type `V`.
/// Requires a `Permit<K>` for the key type.
public fun exists_with_type<K: copy + drop, V: drop>(_: Permit<K>, key: K, _: &TxContext): bool {
    exists_with_type_impl<V>(hash_type_and_key(key))
}

// === Macro Functions ===

/// A wrapper for `add` that constructs the `Permit<$K>` directly.
/// Aborts with `EEntryAlreadyExists` if there is already an entry for `$key`, regardless of its
/// value type.
public macro fun internal_add<$K: copy + drop, $V: drop>(
    $key: $K,
    $value: $V,
    $ctx: &mut TxContext,
) {
    add(permit(internal::permit<$K>()), $key, $value, $ctx)
}

/// A wrapper for `read` that constructs the `Permit<$K>` directly.
/// Aborts with `EEntryDoesNotExist` if there is no entry for `$key`.
/// Aborts with `EEntryTypeMismatch` if the entry exists, but its value is not of type `$V`.
public macro fun internal_read<$K: copy + drop, $V: drop>($key: $K, $ctx: &TxContext): $V {
    read<$K, $V>(permit(internal::permit<$K>()), $key, $ctx)
}

/// A wrapper for `remove` that constructs the `Permit<$K>` directly.
/// Aborts with `EEntryDoesNotExist` if there is no entry for `$key`.
/// Aborts with `EEntryTypeMismatch` if the entry exists, but its value is not of type `$V`.
public macro fun internal_remove<$K: copy + drop, $V: drop>($key: $K, $ctx: &mut TxContext): $V {
    remove<$K, $V>(permit(internal::permit<$K>()), $key, $ctx)
}

/// A wrapper for `exists` that constructs the `Permit<$K>` directly.
public macro fun internal_exists<$K: copy + drop>($key: $K, $ctx: &TxContext): bool {
    exists(permit(internal::permit<$K>()), $key, $ctx)
}

/// A wrapper for `exists_with_type` that constructs the `Permit<$K>` directly.
public macro fun internal_exists_with_type<$K: copy + drop, $V: drop>(
    $key: $K,
    $ctx: &TxContext,
): bool {
    exists_with_type<$K, $V>(permit(internal::permit<$K>()), $key, $ctx)
}

/// Hashes the type and value of `k` against `DUMMY_ROOT` to produce the address identifying its
/// scratch entry.
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
