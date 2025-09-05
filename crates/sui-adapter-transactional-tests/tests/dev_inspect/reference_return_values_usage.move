// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// attempt to return a reference from a function

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    public struct S has key, store {
        id: UID,
        f: u64,
    }

    public fun new(ctx: &mut TxContext): S {
        S { id: object::new(ctx), f: 42 }
    }

    public fun delete(s: S) {
        let S { id, .. } = s;
        object::delete(id);
    }

    public fun borrow_f_mut(s: &mut S): &mut u64 {
        &mut s.f
    }

    public fun borrow_f(s: &S): &u64 {
        &s.f
    }

    public fun inc(u: &mut u64) {
        *u = *u + 1;
    }

    public fun read(u: &u64): u64 {
        *u
    }

    public fun check(u: &u64, expected: u64) {
        assert!(*u == expected, 0);
    }
}

// read from a returned reference
//# programmable --sender A --dev-inspect --inputs 42
//> 0: test::m::new();
//> 1: test::m::borrow_f(Result(0));
//> 2: test::m::read(Result(1));
//> test::m::check(Result(1), Result(2));
//> test::m::check(Result(1), Input(0));
//> test::m::delete(Result(0));

// read from a returned reference, with "subtyping"
//# programmable --sender A --dev-inspect --inputs 42
//> 0: test::m::new();
//> 1: test::m::borrow_f_mut(Result(0));
//> 2: test::m::read(Result(1));
//> test::m::check(Result(1), Result(2));
//> test::m::check(Result(1), Input(0));
//> test::m::delete(Result(0));


// Read from the struct again to check that the reference was actually
// updated, and not just `Result(1)`
//# programmable --sender A --dev-inspect --inputs 43
//> 0: test::m::new();
//> 1: test::m::borrow_f_mut(Result(0));
//> 2: test::m::inc(Result(1));
//> 3: test::m::borrow_f(Result(0));
//> test::m::check(Result(3), Input(0));
//> test::m::delete(Result(0));
