// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `DenyList` type. The `DenyList` shared object is used to restrict access to
/// instances of certain core types from being used as inputs by specified addresses in the "deny
/// list".
module sui::deny_list {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use sui::transfer;
    use std::vector;
    use sui::tx_context;
    use sui::table::{Self, Table};
    use sui::vec_set::{Self, VecSet};
    use sui::versioned::{Self, Versioned};

    friend sui::coin;

    /// Trying to create a deny list object when not called by the system address.
    const ENotSystemAddress: u64 = 0;
    /// The specified address to be removed is not already in the deny list.
    const ENotDenied: u64 = 1;

    #[allow(unused_const)]
    /// The index into the deny list vector for the `sui::coin::Coin` type.
    const COIN_INDEX: u64 = 0;


    /// A shared object that stores the addresses that are blocked for a given core type.
    struct DenyList has key {
        id: UID,
        /// the versioned deny list
        inner: Versioned,
    }

    /// Stores the deny lists for each type
    struct DenyListV0 has store {
        /// A vector of lists, each element is used for a distinct core framework type
        lists: vector<PerTypeListV0>,
    }

    /// Stores the addresses that are denied for a given core type.
    struct PerTypeListV0 has store {
        /// Number of object types that have been banned for a given address.
        /// Used to quickly skip checks for most addresses
        denied_count: Table<address, u64>,
        /// Set of addresses that are banned for a given type.
        /// For example with `sui::coin::Coin`: If addresses A and B are banned from using
        /// "0...0123::my_coin::MY_COIN", this will be "0...0123::my_coin::MY_COIN" -> {A, B}
        denied_addresses: Table<vector<u8>, VecSet<address>>,
    }

    /// Adds the given address to the deny list of the specified type, preventing it
    /// from interacting with instances of that type as an input to a transaction. For coins,
    /// the type specified is the type of the coin, not the coin type itself. For example,
    /// "00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".
    public(friend) fun add(
        deny_list: &mut DenyList,
        per_type_index: u64,
        type: vector<u8>,
        addr: address,
    ) {
        let deny_list: &mut DenyListV0 = versioned::load_value_mut(&mut deny_list.inner);
        let list = vector::borrow_mut(&mut deny_list.lists, per_type_index);
        if (!table::contains(&list.denied_addresses, type)) {
            table::add(&mut list.denied_addresses, type, vec_set::empty());
        };
        let denied_addresses = table::borrow_mut(&mut list.denied_addresses, type);
        let already_denied = vec_set::contains(denied_addresses, &addr);
        if (already_denied) return;

        vec_set::insert(denied_addresses, addr);
        if (!table::contains(&list.denied_count, addr)) {
            table::add(&mut list.denied_count, addr, 0);
        };
        let denied_count = table::borrow_mut(&mut list.denied_count, addr);
        *denied_count = *denied_count + 1;
    }

    /// Removes a previously denied address from the list.
    /// Aborts with `ENotDenied` if the address is not on the list.
    public(friend) fun remove(
        deny_list: &mut DenyList,
        per_type_index: u64,
        type: vector<u8>,
        addr: address,
    ) {
        let deny_list: &mut DenyListV0 = versioned::load_value_mut(&mut deny_list.inner);
        let list = vector::borrow_mut(&mut deny_list.lists, per_type_index);
        let denied_addresses = table::borrow_mut(&mut list.denied_addresses, type);
        assert!(vec_set::contains(denied_addresses, &addr), ENotDenied);
        vec_set::remove(denied_addresses, &addr);
        let denied_count = table::borrow_mut(&mut list.denied_count, addr);
        *denied_count = *denied_count - 1;
        if (*denied_count == 0) {
            table::remove(&mut list.denied_count, addr);
        }
    }

    /// Returns true iff the given address is denied for the given type.
    public(friend) fun contains(
        deny_list: &DenyList,
        per_type_index: u64,
        type: vector<u8>,
        addr: address,
    ): bool {
        let deny_list: &DenyListV0 = versioned::load_value(&deny_list.inner);
        let list = vector::borrow(&deny_list.lists, per_type_index);
        if (!table::contains(&list.denied_count, addr)) return false;

        let denied_count = table::borrow(&list.denied_count, addr);
        if (*denied_count == 0) return false;

        if (!table::contains(&list.denied_addresses, type)) return false;

        let denied_addresses = table::borrow(&list.denied_addresses, type);
        vec_set::contains(denied_addresses, &addr)
    }

    #[allow(unused_function)]
    /// Creation of the deny list object is restricted to the system address
    /// via a system transaction.
    fun create(ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);

        let v0 = DenyListV0 {
            lists: vector[per_type_list(ctx)],
        };
        let inner = versioned::create(0, v0, ctx);
        let deny_list_object = DenyList {
            id: object::sui_deny_list_object_id(),
            inner,
        };
        transfer::share_object(deny_list_object);
    }

    fun per_type_list(ctx: &mut TxContext): PerTypeListV0 {
        PerTypeListV0 {
            denied_count: table::new(ctx),
            denied_addresses: table::new(ctx),
        }
    }

    #[test_only]
    public fun create_for_test(ctx: &mut TxContext) {
        create(ctx);
    }
}
