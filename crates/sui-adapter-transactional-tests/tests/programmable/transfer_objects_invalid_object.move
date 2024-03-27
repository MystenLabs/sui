// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid operations involving TransferObjects of invalid objects

//# init --addresses test=0x0 --accounts A B

//# publish
module test::m1 {
    public struct Pub has key, store {
        id: UID,
        value: u64,
    }

    public struct Cap {}

    public struct Cup<T> has key, store {
        id: UID,
        value: T,
    }

    public fun new(ctx: &mut TxContext): Pub {
        Pub { id: object::new(ctx), value: 112 }
    }

    public fun cup<T>(value: T, ctx: &mut TxContext): Cup<T> {
        Cup { id: object::new(ctx), value }
    }

    public fun cap(): Cap {
        Cap {}
    }
}

// no objects
//# programmable --sender A --inputs @A
//> TransferObjects([], Input(0));

// not an object
//# programmable --sender A --inputs @A
//> 0: test::m1::cap();
//> TransferObjects([Result(0)], Input(0));

// not an object (but sneaky)
//# programmable --sender A --inputs @A
//> 0: test::m1::cap();
// Cup<Cap> is not an object since Cap does not have store
//> 1: test::m1::cup<test::m1::Cap>(Result(0));
//> TransferObjects([Result(1)], Input(0));

// one object, one not
//# programmable --sender A --inputs @A
//> 0: test::m1::new();
//> 1: test::m1::cap();
//> TransferObjects([Result(0), Result(1)], Input(0));

// one object, one not (but sneaky)
//# programmable --sender A --inputs @A
//> 0: test::m1::new();
//> 1: test::m1::cap();
// Cup<Cap> is not an object since Cap does not have store
//> 2: test::m1::cup<test::m1::Cap>(Result(0));
//> TransferObjects([Result(0), Result(2)], Input(0));
