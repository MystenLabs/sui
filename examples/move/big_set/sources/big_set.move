// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A BigSet is a VecSet-like which can go beyond object limit by utilizing
/// dynamic fields. Either by 256 or 65536 times depending on the scaling.
///
/// The structure of this module is similar to the VecSet module, but with some
/// differences:
/// - BigSet requires TxContext due to UID requirement
/// - BigSet can not have `copy` ability, and there's a `copy_` function instead
/// - BigSet can not have `drop` ability, and there's a `drop` function
/// - Returning references to stored values doesn't seem to be possible atm
///
/// The storage structure is as follows:
///
/// 1. When a value is inserted, we hash it and take the first byte (or two)
/// to get the key for the dynamic field.
/// 2. Then we create or add to the VecSet stored in the dynamic field.
/// 3. We also add the key to the VecSet of keys to keep track of all the
/// dynamic fields (for `copy_`, `drop` and `into_keys`).
module big_set::big_set {
    use std::vector;

    use sui::vec_set::{Self, VecSet};
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use sui::dynamic_field as df;
    use sui::hash::blake2b256;
    use sui::bcs;

    /// Error code for unimplemented functions.
    const ENotImplemented: u64 = 0;
    #[allow(unused_const)]
    /// Error code for invalid scaling (currently only 1 and 2 are supported).
    const EInvalidScaling: u64 = 1;
    /// Error code for a key that does not exist in the set.
    const EKeyDoesNotExist: u64 = 2;

    /// An item Key; used to store items in the set.
    struct Key has copy, store, drop { value: vector<u8> }

    /// A Large Collection type which can increase the size of a Collection by
    /// utilizing dynamic fields.
    struct BigSet<phantom K: store + drop> has key, store {
        id: UID,
        /// Length of the key; for 1 byte, the set can hold 256 VecSets in it.
        /// For 2 bytes, the set can hold 65536 sub-sets.
        ///
        /// We don't support more than 2 bytes due to the object size limit.
        scale: u8,
        /// Store the size of the set for faster access.
        size: u64,
        /// Stores currently attached keys.
        keys: VecSet<Key>
    }

    /// Create a new BigSet.
    public fun empty<K: copy + store + drop>(
        scale: u8, ctx: &mut TxContext
    ): BigSet<K> {
        assert!(scale == 1 || scale == 2, EInvalidScaling);
        BigSet {
            scale,
            id: object::new(ctx),
            keys: vec_set::empty(),
            size: 0,
        }
    }

    /// Create a singleton `BigSet` that contains one element.
    public fun singleton<K: copy + store + drop>(
        key: K, scale: u8, ctx: &mut TxContext
    ): BigSet<K> {
        let big_set = empty(scale, ctx);
        insert(&mut big_set, key);
        big_set
    }

    /// Insert a `key` into self.
    /// Aborts if `key` is already present in self.
    public fun insert<K: copy + store + drop>(self: &mut BigSet<K>, key: K) {
        let set_key = key(&key, self.scale);
        if (df::exists_(&self.id, set_key)) {
            let vec_set = df::borrow_mut(&mut self.id, set_key);
            vec_set::insert(vec_set, key);
        } else {
            let vec_set = vec_set::singleton(key);
            df::add(&mut self.id, set_key, vec_set);
            vec_set::insert(&mut self.keys, set_key);
        };

        self.size = self.size + 1;
    }

    /// Remove the entry `key` from self. Aborts if `key` is not present in self.
    public fun remove<K: copy + store + drop>(self: &mut BigSet<K>, key: &K) {
        let set_key = key(key, self.scale);

        assert!(
            df::exists_with_type<Key, VecSet<K>>(&self.id, set_key),
            EKeyDoesNotExist
        );

        let set_mut = df::borrow_mut(&mut self.id, set_key);
        vec_set::remove(set_mut, key);

        if (vec_set::size(set_mut) == 0) {
            let _: VecSet<K> = df::remove(&mut self.id, set_key);
            vec_set::remove(&mut self.keys, &set_key);
        };

        self.size = self.size - 1;
    }

    /// Returns true if `key` is present in self.
    public fun contains<K: copy + store + drop>(
        self: &BigSet<K>, key: &K
    ): bool {
        let set_key = key(key, self.scale);
        df::exists_with_type<Key, VecSet<K>>(&self.id, set_key)
            && vec_set::contains(df::borrow(&self.id, set_key), key)
    }

    /// Returns the number of entries in self.
    public fun size<K: copy + store + drop>(self: &BigSet<K>): u64 {
        self.size
    }

    /// Returns true if self is empty.
    public fun is_empty<K: copy + store + drop>(self: &BigSet<K>): bool {
        self.size == 0
    }

    /// TODO: Not implemented; potentially not implementable.
    public fun keys<K: copy + store + drop>(_self: &BigSet<K>): &vector<K> {
        abort ENotImplemented
    }

    /// Unpacks `BigSet` into a vector of all ever stored keys.
    public fun into_keys<K: copy + store + drop>(self: BigSet<K>): vector<K> {
        let BigSet { id, keys: set_keys, size, scale: _ } = self;
        let result_keys = vector[];

        if (size == 0) {
            object::delete(id);
            return result_keys
        };

        let set_keys = vec_set::into_keys(set_keys);
        while (vector::length(&set_keys) > 0) {
            let set_key = vector::pop_back(&mut set_keys);
            let set = df::remove(&mut id, set_key);
            let keys = vec_set::into_keys(set);
            vector::append(&mut result_keys, keys);
        };

        object::delete(id);
        result_keys
    }

    /// Copy the `BigSet` into a new `BigSet` following `copy` ability of the
    /// simple `VecSet`.
    public fun copy_<K: copy + store + drop>(
        self: &BigSet<K>, ctx: &mut TxContext
    ): BigSet<K> {
        let new_set = empty(self.scale, ctx);
        let set_keys = *vec_set::keys(&self.keys);

        new_set.keys = self.keys;
        new_set.size = self.size;

        while (vector::length(&set_keys) > 0) {
            let set_key = vector::pop_back(&mut set_keys);
            let set_copy: VecSet<K> = *df::borrow(&self.id, set_key);
            df::add(&mut new_set.id, set_key, set_copy);
        };

        new_set
    }

    /// Drop the `BigSet` following `drop` ability of the simple `VecSet`.
    public fun drop<K: copy + store + drop>(self: BigSet<K>) {
        let BigSet { id, keys: set_keys, size, scale: _ } = self;
        if (size == 0) {
            object::delete(id);
            return ()
        };

        let set_keys = vec_set::into_keys(set_keys);
        while (vector::length(&set_keys) > 0) {
            let set_key = vector::pop_back(&mut set_keys);
            let _: VecSet<K> = df::remove(&mut id, set_key);
        };

        object::delete(id)
    }

    /// Generate a Key for the given value.
    fun key<K>(k: &K, scale: u8): Key {
        let value = if (scale == 1) {
            let bytes = blake2b256(&bcs::to_bytes(k));
            vector[ *vector::borrow(&bytes, 0) ]
        } else if (scale == 2) {
            let bytes = blake2b256(&bcs::to_bytes(k));
            vector[
                *vector::borrow(&bytes, 0),
                *vector::borrow(&bytes, 1)
            ]
        } else {
            abort 0
        };

        Key { value }
    }
}
