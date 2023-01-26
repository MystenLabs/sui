// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::prover {

    use sui::object;

    const OWNED: u64 = 1;
    const SHARED: u64 = 2;
    const IMMUTABLE: u64 = 3;


    #[verify_only]
    /// Ownership information for a given object (stored at the object's address)
    struct Ownership has key {
        owner: address, // only matters if status == OWNED
        status: u64,
    }

    #[verify_only]
    /// List of fields with a given name type of an object containing fields (stored at the
    /// containing object's address)
    struct DynamicFields<K: copy + drop + store> has key {
        names: vector<K>,
    }

    #[verify_only]
    /// Information about which object contains a given object field (stored at the field object's
    /// address).
    struct DynamicFieldContainment has key {
        container: address,
    }

    // "public" functions to be used in specs as an equivalent of core Prover's builtins

    /// Verifies if a given object it owned.
    spec fun owned<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).status == OWNED
    }

    /// Verifies if a given object is owned.
    spec fun owned_by<T: key>(obj: T, owner: address): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).status == OWNED &&
        global<Ownership>(addr).owner == owner
    }

    /// Verifies if a given object is shared.
    spec fun shared<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).status == SHARED
    }

    /// Verifies if a given object is immutable.
    spec fun immutable<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).status == IMMUTABLE
    }

    /// Verifies if a given object has field with a given name.
    spec fun has_field<T: key, K: copy + drop + store>(obj: T, name: K): bool {
        let addr = object::id(obj).bytes;
        uid_has_field<K>(addr, name)
    }

    /// Returns number of K-type fields of a given object.
    spec fun num_fields<T: key, K: copy + drop + store>(obj: T): u64 {
        let addr = object::id(obj).bytes;
        if (!exists<DynamicFields<K>>(addr)) {
            0
        } else {
            len(global<DynamicFields<K>>(addr).names)
        }
    }

    // "helper" function - may also be used in specs but mostly opaque ones defining behavior of key
    // framework functions

    spec fun uid_has_field<K: copy + drop + store>(addr: address, name: K): bool {
        exists<DynamicFields<K>>(addr) && contains(global<DynamicFields<K>>(addr).names, name)
    }

    // remove an element at index from a vector and return the resulting vector
    spec fun vec_remove<T>(v: vector<T>, elem_idx: u64, current_idx: u64) : vector<T> {
        let len = len(v);
        if (current_idx != len) {
            vec()
        } else if (current_idx != elem_idx) {
            concat(vec(v[current_idx]), vec_remove(v, elem_idx, current_idx + 1))
        } else {
            vec_remove(v, elem_idx, current_idx + 1)
        }
    }

}
