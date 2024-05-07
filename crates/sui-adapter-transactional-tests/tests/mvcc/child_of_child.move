// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests accessing the versions of a child of a child

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    use sui::dynamic_object_field as ofield;

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
        ofield::add(&mut parent.id, KEY, child);
        ofield::add(&mut grand.id, KEY, parent);
        grand
    }

    //////////////////////////////////////////////////////////////
    // set

    public fun set(grand: &mut Obj, v1: u64, v2: u64, v3: u64) {
        grand.value = v1;
        let parent: &mut Obj = ofield::borrow_mut(&mut grand.id, KEY);
        parent.value = v2;
        let child: &mut Obj = ofield::borrow_mut(&mut parent.id, KEY);
        child.value = v3;
    }

    //////////////////////////////////////////////////////////////
    // remove

    public fun remove(grand: &mut Obj) {
        let parent: &mut Obj = ofield::borrow_mut(&mut grand.id, KEY);
        let Obj { id, value: _ } = ofield::remove(&mut parent.id, KEY);
        object::delete(id);
    }

    //////////////////////////////////////////////////////////////
    // check

    public fun check(grand: &Obj, v1: u64, v2: u64, v3: Option<u64>) {
        assert!(grand.value == v1, 0);
        let parent: &Obj = ofield::borrow(&grand.id, KEY);
        assert!(parent.value == v2, 0);
        if (option::is_some(&v3)) {
            let child: &Obj = ofield::borrow(&parent.id, KEY);
            assert!(&child.value == option::borrow(&v3), 0);
        } else {
            assert!(!ofield::exists_<u64>(&parent.id, KEY), 0);
        }
    }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> TransferObjects([Result(0)], Input(0))

//# view-object 2,4

//# programmable --sender A --inputs object(2,4) 1 2 3
//> test::m::set(Input(0), Input(1), Input(2), Input(3))

//# view-object 2,4

//# programmable --sender A --inputs object(2,4)
//> test::m::remove(Input(0))

//# view-object 2,4


// dev-inspect with 'check' and correct values

//# programmable --sender A --inputs object(2,4)@2 0 0 vector[0] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,4)@3 1 2 vector[3] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,4)@4 1 2 vector[] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))


// dev-inspect with 'check' and _incorrect_ values

//# programmable --sender A --inputs object(2,4)@3 0 0 vector[0] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,4)@4 1 2 vector[3] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,4)@2 1 2 vector[] --dev-inspect
//> test::m::check(Input(0), Input(1), Input(2), Input(3))
