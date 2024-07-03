// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests accessing version of the input parent, not the runtime parent

//# init --addresses test=0x0 --accounts P1 P2 --protocol-version 16

//# publish

module test::m {
    use sui::dynamic_field as field;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    const KEY: u64 = 0;

    public fun a(ctx: &mut TxContext): A {
        A { id: object::new(ctx) }
    }

    public fun b(ctx: &mut TxContext): B {
        let mut b = B { id: object::new(ctx) };
        field::add(&mut b.id, KEY, 0);
        b

    }

    public fun bump(b: &mut B) {
        let f = field::borrow_mut(&mut b.id, KEY);
        *f = *f + 1;
    }

    public fun append(a: &mut A, b: B) {
        field::add(&mut a.id, KEY, b);
    }

    public fun check(a: &A, expected: u64) {
        let b: &B = field::borrow(&a.id, KEY);
        let v = *field::borrow(&b.id, KEY);
        assert!(v == expected, 0);
    }

    public fun nop() {
    }
}

// Create object A at version 2

//# programmable --sender P1 --inputs @P1
//> 0: test::m::a();
//> TransferObjects([Result(0)], Input(0))


//# view-object 2,0


// Create object B with a version 2 and 3 for it's dynamic field

//# programmable --sender P2 --inputs @P2
//> 0: test::m::b();
//> TransferObjects([Result(0)], Input(0))

//# view-object 4,0

//# programmable --sender P2 --inputs object(4,1)
//> 0: test::m::bump(Input(0));

//# view-object 4,0

// Append object B to object A. And ensure that when we later read the dynamic
// field of object B, we get the most recent version.

//# programmable --sender P2 --inputs object(2,0)@2 object(4,1)@3 1 --dev-inspect
//> 0: test::m::append(Input(0), Input(1));
//> 1: test::m::check(Input(0), Input(2));

// checking that with version 3 we get the other value, then flip them to ensure
// they abort

//# programmable --sender P2 --inputs object(2,0)@2 object(4,1)@2 0 --dev-inspect
//> 0: test::m::append(Input(0), Input(1));
//> 1: test::m::check(Input(0), Input(2));

// @2 with value 1 aborts

//# programmable --sender P2 --inputs object(2,0)@2 object(4,1)@2 1 --dev-inspect
//> 0: test::m::append(Input(0), Input(1));
//> 1: test::m::check(Input(0), Input(2));

// @3 with value 0 aborts

//# programmable --sender P2 --inputs object(2,0)@2 object(4,1)@3 0 --dev-inspect
//> 0: test::m::append(Input(0), Input(1));
//> 1: test::m::check(Input(0), Input(2));
