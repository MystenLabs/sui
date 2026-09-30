// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A reference result cannot flow into a value position unless the referenced type has `copy`.
// `SplitCoins` amounts and `TransferObjects` recipients are always `u64` and `address`, which
// have `copy`, so they cannot be tested here.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    use sui::coin::Coin;
    use sui::sui::SUI;

    public struct NoCopy has drop, store {
        value: u64,
    }

    public struct Obj has key, store {
        id: UID,
        inner: NoCopy,
    }

    public fun id_ref<T>(x: &T): &T {
        x
    }

    public fun borrow_mut<T>(x: &mut T): &mut T {
        x
    }

    public fun no_copy(value: u64): NoCopy {
        NoCopy { value }
    }

    public fun borrow_both(a: &NoCopy, b: &NoCopy): (&NoCopy, &NoCopy) {
        (a, b)
    }

    public fun new_obj(value: u64, ctx: &mut TxContext): Obj {
        Obj { id: object::new(ctx), inner: NoCopy { value } }
    }

    public fun borrow_inner(o: &Obj): &NoCopy {
        &o.inner
    }

    public fun take(_: NoCopy) {}

    public fun take_vec(_: vector<NoCopy>) {}

    public fun take_coin(c: Coin<SUI>, ctx: &TxContext) {
        transfer::public_transfer(c, ctx.sender())
    }
}

// Move call value parameter

//# programmable --sender A --inputs 0u64
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::id_ref<test::m::NoCopy>(Result(0));
//> 2: test::m::take(Result(1));

// Move call value parameter, through a mutable reference

//# programmable --sender A --inputs 0u64
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::borrow_mut<test::m::NoCopy>(Result(0));
//> 2: test::m::take(Result(1));

// Nested result

//# programmable --sender A --inputs 0u64 1u64
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::no_copy(Input(1));
//> 2: test::m::borrow_both(Result(0), Result(1));
//> 3: test::m::take(NestedResult(2,1));

// Reference into an object input

//# programmable --sender A --inputs 0u64 @A
//> 0: test::m::new_obj(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(5,0)
//> 0: test::m::borrow_inner(Input(0));
//> 1: test::m::take(Result(0));

// MakeMoveVec element

//# programmable --sender A --inputs 0u64
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::id_ref<test::m::NoCopy>(Result(0));
//> 2: MakeMoveVec<test::m::NoCopy>([Result(1)]);
//> 3: test::m::take_vec(Result(2));

// TransferObjects object. Objects can never have `copy`, so this will fail before attempting to
// generate a `Read`

//# programmable --sender A --inputs 0u64 @A
//> 0: test::m::new_obj(Input(0));
//> 1: test::m::id_ref<test::m::Obj>(Result(0));
//> TransferObjects([Result(1)], Input(1))
