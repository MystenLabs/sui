// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::prover {

    use std::option;
    use std::vector;
    use sui::object;

    const OWNED: u64 = 1;
    const SHARED: u64 = 2;
    const IMMUTABLE: u64 = 3;


    #[verify_only]
    struct Ownership has key {
        owner: option::Option<address>,
        status: u64,
    }

    spec fun owned<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        option::is_some(global<Ownership>(addr).owner) &&
        global<Ownership>(addr).status == OWNED
    }

    spec fun owned_by<T: key>(obj: T, owner: address): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).owner == option::spec_some(owner) &&
        global<Ownership>(addr).status == OWNED
    }

        /*
    spec fun is_field_of<T: key>(obj: T, owner: sui::object::UID): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        global<Ownership>(addr).owner == option::spec_some(owner) &&
        global<Ownership>(addr).status == OWNED
}
        */

    spec fun shared<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        option::is_none(global<Ownership>(addr).owner) &&
        global<Ownership>(addr).status == SHARED
    }

    spec fun immutable<T: key>(obj: T): bool {
        let addr = object::id(obj).bytes;
        exists<Ownership>(addr) &&
        option::is_none(global<Ownership>(addr).owner) &&
        global<Ownership>(addr).status == IMMUTABLE
    }

    #[verify_only]
    struct DynamicFields<K: copy + drop + store> has key {
        names: vector<K>,
    }

    spec fun uid_has_field<K: copy + drop + store>(addr: address, name: K): bool {
        exists<DynamicFields<K>>(addr) && contains(global<DynamicFields<K>>(addr).names, name)
    }

    spec fun has_field<T: key, K: copy + drop + store>(obj: T, name: K): bool {
        let addr = object::id(obj).bytes;
        uid_has_field<K>(addr, name)
    }

    spec fun always_true<K: copy + drop + store>(): bool {
        exists<DynamicFields<K>>(@0x42) || !exists<DynamicFields<K>>(@0x42)
    }

}
