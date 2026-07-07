// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Basic end-to-end coverage of `sui::scratch` through "real" transaction execution, and a check
// that the store is cleared between transactions (a fresh `ScratchRuntime` per transaction).

//# init --addresses test=0x0 --accounts A

//# publish
module test::scratch_test {
    public struct Marker() has copy, drop;

    // Full add / read / remove lifecycle within a single transaction.
    public entry fun add_read_remove(ctx: &mut TxContext) {
        assert!(!sui::scratch::internal_exists!<Marker>(ctx, Marker()), 0);
        sui::scratch::internal_add!<Marker, u64>(ctx, Marker(), 42);
        assert!(sui::scratch::internal_exists!<Marker>(ctx, Marker()), 1);
        assert!(sui::scratch::internal_exists_with_type!<Marker, u64>(ctx, Marker()), 2);
        assert!(sui::scratch::internal_read!<Marker, u64>(ctx, Marker()) == 42, 3);
        assert!(sui::scratch::internal_remove!<Marker, u64>(ctx, Marker()) == 42, 4);
        assert!(!sui::scratch::internal_exists!<Marker>(ctx, Marker()), 5);
    }

    // Adds an entry and leaves it in the store, so the next transaction can observe whether it
    // persisted.
    public entry fun add_only(ctx: &mut TxContext) {
        sui::scratch::internal_add!<Marker, u64>(ctx, Marker(), 42);
    }

    // Aborts if a `Marker` entry exists. Used to prove the store was not persisted between
    // transactions.
    public entry fun assert_absent(ctx: &mut TxContext) {
        assert!(!sui::scratch::internal_exists!<Marker>(ctx, Marker()), 6);
    }
}

//# programmable --sender A
// Basic coverage: add/read/remove all succeed within one transaction.
//> test::scratch_test::add_read_remove();

//# programmable --sender A
// Add an entry in this transaction and leave it in the store to be dropped.
//> test::scratch_test::add_only();

//# programmable --sender A
// A fresh transaction cannot see the previous transaction's entry.
//> test::scratch_test::assert_absent();
