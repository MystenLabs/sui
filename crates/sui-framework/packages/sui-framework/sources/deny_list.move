// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `DenyList` type. The `DenyList` shared object is used to restrict access to
/// instances of certain core types from being used as inputs by specified addresses in the deny
/// list.
module sui::deny_list {
    use sui::table::{Self, Table};
    use sui::bag::{Self, Bag};
    use sui::vec_set::{Self, VecSet};

    /// Trying to create a deny list object when not called by the system address.
    const ENotSystemAddress: u64 = 0;
    /// The specified address to be removed is not already in the deny list.
    const ENotDenied: u64 = 1;

    /// The index into the deny list vector for the `sui::coin::Coin` type.
    const COIN_INDEX: u64 = 0;

    /// A shared object that stores the addresses that are blocked for a given core type.
    public struct DenyList has key {
        id: UID,
        /// The individual deny lists.
        lists: Bag,
    }

    /// Stores the addresses that are denied for a given core type.
    public struct PerTypeList has key, store {
        id: UID,
        /// Number of object types that have been banned for a given address.
        /// Used to quickly skip checks for most addresses.
        denied_count: Table<address, u64>,
        /// Set of addresses that are banned for a given type.
        /// For example with `sui::coin::Coin`: If addresses A and B are banned from using
        /// "0...0123::my_coin::MY_COIN", this will be "0...0123::my_coin::MY_COIN" -> {A, B}.
        denied_addresses: Table<vector<u8>, VecSet<address>>,
    }

    /// Adds the given address to the deny list of the specified type, preventing it
    /// from interacting with instances of that type as an input to a transaction. For coins,
    /// the type specified is the type of the coin, not the coin type itself. For example,
    /// "00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".
    public(package) fun add(
        deny_list: &mut DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ) {
        let bag_entry: &mut PerTypeList = &mut deny_list.lists[per_type_index];
        bag_entry.per_type_list_add(`type`, addr)
    }

    fun per_type_list_add(
        list: &mut PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ) {
        if (!list.denied_addresses.contains(`type`)) {
            list.denied_addresses.add(`type`, vec_set::empty());
        };
        let denied_addresses = &mut list.denied_addresses[`type`];
        let already_denied = denied_addresses.contains(&addr);
        if (already_denied) return;

        denied_addresses.insert(addr);
        if (!list.denied_count.contains(addr)) {
            list.denied_count.add(addr, 0);
        };
        let denied_count = &mut list.denied_count[addr];
        *denied_count = *denied_count + 1;
    }

    /// Removes a previously denied address from the list.
    /// Aborts with `ENotDenied` if the address is not on the list.
    public(package) fun remove(
        deny_list: &mut DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ) {
        per_type_list_remove(&mut deny_list.lists[per_type_index], `type`, addr)
    }

    fun per_type_list_remove(
        list: &mut PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ) {
        let denied_addresses = &mut list.denied_addresses[`type`];
        assert!(denied_addresses.contains(&addr), ENotDenied);
        denied_addresses.remove(&addr);
        let denied_count = &mut list.denied_count[addr];
        *denied_count = *denied_count - 1;
        if (*denied_count == 0) {
            list.denied_count.remove(addr);
        }
    }

    /// Returns true iff the given address is denied for the given type.
    public(package) fun contains(
        deny_list: &DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ): bool {
        per_type_list_contains(&deny_list.lists[per_type_index], `type`, addr)
    }

    fun per_type_list_contains(
        list: &PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ): bool {
        if (!list.denied_count.contains(addr)) return false;

        let denied_count = &list.denied_count[addr];
        if (*denied_count == 0) return false;

        if (!list.denied_addresses.contains(`type`)) return false;

        let denied_addresses = &list.denied_addresses[`type`];
        denied_addresses.contains(&addr)
    }

    #[allow(unused_function)]
    /// Creation of the deny list object is restricted to the system address
    /// via a system transaction.
    fun create(ctx: &mut TxContext) {
        assert!(ctx.sender() == @0x0, ENotSystemAddress);

        let mut lists = bag::new(ctx);
        lists.add(COIN_INDEX, per_type_list(ctx));
        let deny_list_object = DenyList {
            id: object::sui_deny_list_object_id(),
            lists,
        };
        transfer::share_object(deny_list_object);
    }

    fun per_type_list(ctx: &mut TxContext): PerTypeList {
        PerTypeList {
            id: object::new(ctx),
            denied_count: table::new(ctx),
            denied_addresses: table::new(ctx),
        }
    }

    #[test_only]
    public fun create_for_test(ctx: &mut TxContext) {
        create(ctx);
    }

    #[test_only]
    /// Creates and returns a new DenyList object for testing purposes. It
    /// doesn't matter which object ID the list has in this kind of test.
    public fun new_for_testing(ctx: &mut TxContext): DenyList {
        let mut lists = bag::new(ctx);
        lists.add(COIN_INDEX, per_type_list(ctx));
        DenyList {
            id: object::new(ctx),
            lists,
        }
    }
}
