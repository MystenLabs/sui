// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Generated `Read`s require the dereferenced type to have `copy`.
// Generated `Read`s require the underlying reference to still be live (i.e. not moved).

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    public struct Obj has key, store {
        id: UID,
        value: u64,
    }

    public fun id_ref<T>(x: &T): &T {
        x
    }

    public fun new_obj(value: u64, ctx: &mut TxContext): Obj {
        Obj { id: object::new(ctx), value }
    }

    public fun borrow_value(o: &Obj): &u64 {
        &o.value
    }

    public fun take(o: Obj, ctx: &TxContext) {
        transfer::public_transfer(o, ctx.sender())
    }

    public fun check(value: u64, expected: u64) {
        assert!(value == expected, 0);
    }
}

// Read of a non-copy type

//# programmable --sender A --inputs 112u64
//> 0: test::m::new_obj(Input(0));
//> 1: test::m::id_ref<test::m::Obj>(Result(0));
//> 2: test::m::take(Result(1));

// Referent moved while a reference into it is still read later

//# programmable --sender A --inputs 112u64
//> 0: test::m::new_obj(Input(0));
//> 1: test::m::borrow_value(Result(0));
//> 2: test::m::take(Result(0));
// without this call to `check`, the test would pass since the reference
// `Result(1)` would be automatically released
//> 3: test::m::check(Result(1), Input(0));
