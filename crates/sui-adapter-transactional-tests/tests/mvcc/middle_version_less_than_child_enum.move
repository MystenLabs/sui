// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests accessing the versions of a child of a child using enums

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    use sui::dynamic_field as field;

    public struct Obj has key, store {
        id: UID,
        value: u64,
    }

    public enum Enum has store {
        V {
            id: UID,
            value: u64,
        }
    }

    const KEY: u64 = 0;

    //////////////////////////////////////////////////////////////
    // new

    public fun new(ctx: &mut TxContext): Obj {
        let mut grand = Obj { id: object::new(ctx), value: 0 };
        let mut parent = Enum::V { id: object::new(ctx), value: 0 };
        let child = Enum::V { id: object::new(ctx), value: 0 };
        match (&mut parent) {
            Enum::V { id, .. } => {
                field::add(id, KEY, child);
            }
        };
        field::add(&mut grand.id, KEY, parent);
        grand
    }

    //////////////////////////////////////////////////////////////
    // set

    public fun set(grand: &mut Obj, v: u64) {
        let parent: &mut Enum = field::borrow_mut(&mut grand.id, KEY);
        match (parent) {
            Enum::V { id, ..} => {
                match (field::borrow_mut(id, KEY)) {
                    Enum::V { value, .. } => {
                        *value = v;
                    }
                }
            }
        };
    }

    //////////////////////////////////////////////////////////////
    // check

    public fun check(grand: &Obj, expected: u64) {
        assert!(grand.value == 0, 0);
        let parent: &Enum = field::borrow(&grand.id, KEY);
        match (parent) {
            Enum::V { id, value } => {
                assert!(value == 0, 0);
                match (field::borrow(id, KEY)) {
                    Enum::V { value, .. } => {
                        assert!(value == expected, 0);
                    }
                }
            }
        };
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
