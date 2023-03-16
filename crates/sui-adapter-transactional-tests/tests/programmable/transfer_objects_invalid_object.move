// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tessts various invalid vector instantions for types

//# init --addresses test=0x0 --accounts A B

//# publish
module test::m1 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Pub has key, store {
        id: UID,
        value: u64,
    }

    struct Cap {}

    struct Cup<T> has key, store {
        id: UID,
        p: Phantom<T>,
    }
    struct Phantom<phantom T> has copy, drop, store {}

    public fun new(ctx: &mut TxContext): Pub {
        Pub { id: object::new(ctx), value: 112 }
    }

    public fun cup<T>(ctx: &mut TxContext): Cup<T> {
        Cup { id: object::new(ctx), p: Phantom {} }
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
//> 0: test::m1::cup<signer>(); // not an object since signer does not have store
//> TransferObjects([Result(0)], Input(0));

// one object, one not
//# programmable --sender A --inputs @A
//> 0: test::m1::new();
//> 1: test::m1::cap();
//> TransferObjects([Result(0), Result(1)], Input(0));

// one object, one not (but sneaky)
//# programmable --sender A --inputs @A
//> 0: test::m1::new();
//> 1: test::m1::cup<signer>(); // not an object since signer does not have store
//> TransferObjects([Result(0), Result(1)], Input(0));
