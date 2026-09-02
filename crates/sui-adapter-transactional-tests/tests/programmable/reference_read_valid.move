// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Every value position a reference result can flow into that produces a `Read` argument.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    public struct Pair has copy, drop, store {
        x: u64,
        y: u64,
    }

    public struct Obj has key, store {
        id: UID,
        value: u64,
    }

    public fun id_ref<T>(x: &T): &T {
        x
    }

    public fun pair(x: u64, y: u64): Pair {
        Pair { x, y }
    }

    public fun borrow_x(p: &Pair): &u64 {
        &p.x
    }

    public fun borrow_x_y(p: &Pair): (&u64, &u64) {
        (&p.x, &p.y)
    }

    public fun borrow_x_mut(p: &mut Pair): &mut u64 {
        &mut p.x
    }

    public fun write_u64(x: &mut u64) {
        *x = *x + 1;
    }

    public fun new_obj(value: u64, ctx: &mut TxContext): Obj {
        Obj { id: object::new(ctx), value }
    }

    public fun borrow_value(o: &Obj): &u64 {
        &o.value
    }

    public fun check(value: u64, expected: u64) {
        assert!(value == expected, 0);
    }

    public fun check_pair(p: Pair, x: u64, y: u64) {
        assert!(p.x == x && p.y == y, 0);
    }

    public fun check_and_borrow_y(x: u64, p: &Pair): &u64 {
        assert!(p.x == x, 0);
        &p.y
    }

    public fun check_vec(v: vector<u64>, len: u64, value: u64) {
        assert!(v.length() == len, 0);
        v.do!(|x| assert!(x == value, 0));
    }
}

// Move call value parameter

//# programmable --sender A --inputs 112u64
//> 0: test::m::id_ref<u64>(Input(0));
//> 1: test::m::check(Result(0), Input(0));

// SplitCoins amount

//# programmable --sender A --inputs 100u64 @A
//> 0: test::m::id_ref<u64>(Input(0));
//> 1: SplitCoins(Gas, [Result(0)]);
//> TransferObjects([Result(1)], Input(1))

// TransferObjects recipient

//# programmable --sender A --inputs @A 100u64
//> 0: test::m::id_ref<address>(Input(0));
//> 1: SplitCoins(Gas, [Input(1)]);
//> TransferObjects([Result(1)], Result(0))

// MakeMoveVec element, mixed with a pure value

//# programmable --sender A --inputs 7u64 3u64
//> 0: test::m::id_ref<u64>(Input(0));
//> 1: MakeMoveVec<u64>([Result(0), Input(0), Result(0)]);
//> 2: test::m::check_vec(Result(1), Input(1), Input(0));

// Read through a mutable reference, before and after a write

//# programmable --sender A --inputs 1u64 2u64
//> 0: test::m::pair(Input(0), Input(1));
//> 1: test::m::borrow_x_mut(Result(0));
//> 2: test::m::check(Result(1), Input(0));
//> 3: test::m::write_u64(Result(1));
//> 4: test::m::check(Result(1), Input(1));

// Read of a copyable struct

//# programmable --sender A --inputs 1u64 2u64
//> 0: test::m::pair(Input(0), Input(1));
//> 1: test::m::id_ref<test::m::Pair>(Result(0));
//> 2: test::m::check_pair(Result(1), Input(0), Input(1));

// Read of nested results

//# programmable --sender A --inputs 1u64 2u64
//> 0: test::m::pair(Input(0), Input(1));
//> 1: test::m::borrow_x_y(Result(0));
//> 2: test::m::check(NestedResult(1,0), Input(0));
//> 3: test::m::check(NestedResult(1,1), Input(1));

// Read of a reference into an object input

//# programmable --sender A --inputs 112u64 @A
//> 0: test::m::new_obj(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(9,0) 112u64
//> 0: test::m::borrow_value(Input(0));
//> 1: test::m::check(Result(0), Input(1));

// The same reference read twice in one command, and again in a later command

//# programmable --sender A --inputs 112u64
//> 0: test::m::id_ref<u64>(Input(0));
//> 1: test::m::check(Result(0), Result(0));
//> 2: test::m::check(Result(0), Input(0));

// Read alongside a copy of another reference with the same root, which extends the borrow

//# programmable --sender A --inputs 1u64 2u64
//> 0: test::m::pair(Input(0), Input(1));
//> 1: test::m::id_ref<test::m::Pair>(Result(0));
//> 2: test::m::borrow_x(Result(1));
//> 3: test::m::check_and_borrow_y(Result(2), Result(1));
//> 4: test::m::check(Result(3), Input(1));

// Read of a reference into the referent while the referent itself is also copied

//# programmable --sender A --inputs 1u64 2u64
//> 0: test::m::pair(Input(0), Input(1));
//> 1: test::m::borrow_x(Result(0));
//> 2: test::m::check_pair(Result(0), Result(1), Input(1));
