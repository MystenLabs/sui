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
    use sui::vec_set::VecSet;

    friend sui::coin;

    /// Trying to create a deny list object when not called by the system address.
    const ENotSystemAddress: u64 = 3;

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

    public(friend) fun coin_tables(
        freezer: &Freezer
    ): (&Table<address, u64>, &Table<ID, VecSet<address>>) {
        let coin_freezer = vector::borrow(&freezer.freezers, COIN_INDEX);
        (&coin_freezer.frozen_count, &coin_freezer.frozen_addresses)
    }

    public(friend) fun coin_tables_mut(
        freezer: &mut Freezer
    ): (&mut Table<address, u64>, &mut Table<ID, VecSet<address>>) {
        let coin_freezer = vector::borrow_mut(&mut freezer.freezers, COIN_INDEX);
        (&mut coin_freezer.frozen_count, &mut coin_freezer.frozen_addresses)
    }

    #[allow(unused_function)]
    /// Creation of the freezer object is restricted to the system address via a system transaction.
    fun create_deny_list_object(ctx: &mut TxContext) {
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
    public fun create_deny_list_object_for_test(ctx: &mut TxContext) {
        create_deny_list_object(ctx);
    }
}
