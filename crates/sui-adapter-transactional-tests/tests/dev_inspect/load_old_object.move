// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// load an object at an old version

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    public struct S has key, store {
        id: UID,
        value: u64
    }

    public fun new(ctx: &mut TxContext): S {
        S {
            id: object::new(ctx),
            value: 0,
        }
    }

    public fun set(s: &mut S, value: u64) {
        s.value = value
    }

    public fun check(s: &S, expected: u64) {
        assert!(s.value == expected, 0);
    }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> TransferObjects([Result(0)], Input(0))

//# view-object 2,0

//# programmable --sender A --inputs object(2,0) 112
//> test::m::set(Input(0), Input(1))

//# view-object 2,0


// dev-inspect with 'check' and correct values

//# programmable --sender A --inputs object(2,0)@2 0 --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0)@3 112 --dev-inspect
//> test::m::check(Input(0), Input(1))


// dev-inspect with 'check' and _incorrect_ values

//# programmable --sender A --inputs object(2,0)@2 112 --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0)@3 0 --dev-inspect
//> test::m::check(Input(0), Input(1))
