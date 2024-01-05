// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `Freezer` type. The `Freezer` shared object is used to restrict access to instances
/// of certain core types from being used as inputs by specified "frozen" addresses.
module sui::freezer {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID, ID};
    use sui::transfer;
    use std::vector;
    use sui::tx_context;
    use sui::table::{Self, Table};
    use sui::vec_set::{Self, VecSet};

    friend sui::coin;

    /// Trying to create a deny list object when not called by the system address.
    const ENotSystemAddress: u64 = 0;
    /// The specified address to be unfrozen is not already frozen.
    const ENotFrozen: u64 = 1;

    #[allow(unused_const)]
    /// The index into the freezers vector for the `sui::coin::Coin` type.
    const COIN_INDEX: u64 = 0;


    /// Stores the addresses that are frozen for a given core type.
    struct PerTypeFreezer has store {
        /// Number of object types that have been banned for a given address.
        /// Used to quickly skip frozen checks for most addresses
        frozen_count: Table<address, u64>,
        /// Set of addresses that are banned for a given package ID.
        /// For example with `sui::coin::Coin`: If addresses A and B are banned from using
        /// `0x123::my_coin::MY_COIN`, this will be 0x123 -> {A, B}
        frozen_addresses: Table<ID, VecSet<address>>,
    }

    /// A shared object that stores the addresses that are frozen for a given core type.
    struct Freezer has key {
        id: UID,
        /// A vector of freezers, each element is used for a distinct core framework type
        freezers: vector<PerTypeFreezer>,
    }


    /// Freezes the given address, preventing it
    /// from interacting with the coin as an input to a transaction.
    public(friend) fun freeze_address(
        freezer: &mut Freezer,
        per_type_index: u64,
        package: ID,
        addr: address,
    ) {
        let freezer = vector::borrow_mut(&mut freezer.freezers, per_type_index);
        if (!table::contains(&freezer.frozen_addresses, package)) {
            table::add(&mut freezer.frozen_addresses, package, vec_set::empty());
        };
        let frozen_addresses = table::borrow_mut(&mut freezer.frozen_addresses, package);
        let already_frozen = vec_set::contains(frozen_addresses, &addr);
        if (already_frozen) return;

        vec_set::insert(frozen_addresses, addr);
        if (!table::contains(&freezer.frozen_count, addr)) {
            table::add(&mut freezer.frozen_count, addr, 0);
        };
        let frozen_count = table::borrow_mut(&mut freezer.frozen_count, addr);
        *frozen_count = *frozen_count + 1;
    }

    /// Removes a previously frozen address from the freeze list.
    /// Aborts with `ENotFrozen` if the address is not frozen.
    public(friend) fun unfreeze_address(
        freezer: &mut Freezer,
        per_type_index: u64,
        package: ID,
        addr: address,
    ) {
        let freezer = vector::borrow_mut(&mut freezer.freezers, per_type_index);
        let frozen_addresses = table::borrow_mut(&mut freezer.frozen_addresses, package);
        assert!(vec_set::contains(frozen_addresses, &addr), ENotFrozen);
        let frozen_addresses = table::borrow_mut(&mut freezer.frozen_addresses, package);
        vec_set::remove(frozen_addresses, &addr);
        let frozen_count = table::borrow_mut(&mut freezer.frozen_count, addr);
        *frozen_count = *frozen_count - 1;
    }

    /// Returns true iff the given address is frozen for the given coin type. It will
    /// return false if given a non-coin type.
    public(friend) fun address_is_frozen(
        freezer: &Freezer,
        per_type_index: u64,
        package: ID,
        addr: address,
    ): bool {
        let freezer = vector::borrow(&freezer.freezers, per_type_index);
        if (!table::contains(&freezer.frozen_count, addr)) return false;

        let frozen_count = table::borrow(&freezer.frozen_count, addr);
        if (*frozen_count == 0) return false;

        if (!table::contains(&freezer.frozen_addresses, package)) return false;

        let frozen_addresses = table::borrow(&freezer.frozen_addresses, package);
        vec_set::contains(frozen_addresses, &addr)
    }

    #[allow(unused_function)]
    /// Creation of the freezer object is restricted to the system address via a system transaction.
    fun create(ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);

        let deny_list_object = Freezer {
            id: object::sui_deny_list_object_id(),
            freezers: vector[per_type_freezer(ctx)],
        };
        transfer::share_object(deny_list_object);
    }

    fun per_type_freezer(ctx: &mut TxContext): PerTypeFreezer {
        PerTypeFreezer {
            frozen_count: table::new(ctx),
            frozen_addresses: table::new(ctx),
        }
    }

    #[test_only]
    public fun create_for_test(ctx: &mut TxContext) {
        create(ctx);
    }
}
