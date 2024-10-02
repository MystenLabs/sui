// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests accessing the versions of a child of a child

//# init --addresses test=0x0 --accounts A --protocol-version 16

//# publish

module test::m {
    use sui::dynamic_field as field;

    public struct Obj has key, store {
        id: UID,
        value: u64,
    }

    const KEY: u64 = 0;

    //////////////////////////////////////////////////////////////
    // new

    public fun new(ctx: &mut TxContext): Obj {
        let mut grand = Obj { id: object::new(ctx), value: 0 };
        let mut parent = Obj { id: object::new(ctx), value: 0 };
        let child = Obj { id: object::new(ctx), value: 0 };
        field::add(&mut parent.id, KEY, child);
        field::add(&mut grand.id, KEY, parent);
        grand
    }

    //////////////////////////////////////////////////////////////
    // set

    public fun set(grand: &mut Obj, v: u64) {
        let parent: &mut Obj = field::borrow_mut(&mut grand.id, KEY);
        let child: &mut Obj = field::borrow_mut(&mut parent.id, KEY);
        child.value = v;
    }

    //////////////////////////////////////////////////////////////
    // check

    public fun check(grand: &Obj, expected: u64) {
        assert!(grand.value == 0, 0);
        let parent: &Obj = field::borrow(&grand.id, KEY);
        assert!(parent.value == 0, 0);
        let child: &Obj = field::borrow(&parent.id, KEY);
        assert!(child.value == expected, 0);
    }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> TransferObjects([Result(0)], Input(0))

// All 3 objects have version 2

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# programmable --sender A --inputs object(2,2) 112
//> test::m::set(Input(0), Input(1))

// The middle object has version 2, while the root and modified leaf have version 3

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

// correctly load the leaf even though it has a version greater than its immediate
// parent

//# programmable --sender A --inputs object(2,2) 112
//> test::m::check(Input(0), Input(1))
