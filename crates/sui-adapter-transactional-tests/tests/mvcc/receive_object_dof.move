// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0 --accounts A

//# publish
module tto::M1 {
    use sui::transfer::Receiving;
    use sui::dynamic_object_field as dof;

    const KEY: u64 = 0;

    public struct A has key, store {
        id: UID,
        value: u64,
    }


    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx), value: 0 };
        let a_address = object::id_address(&a);
        let mut b = A { id: object::new(ctx), value: 0 };
        dof::add(&mut b.id, KEY, A { id: object::new(ctx), value: 0 });
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }

    public entry fun receive(parent: &mut A, x: Receiving<A>) {
        let b = transfer::receive(&mut parent.id, x);
        dof::add(&mut parent.id, KEY, b);
    }

    public fun set(grand: &mut A, v1: u64, v2: u64, v3: u64) {
        grand.value = v1;
        let parent: &mut A = dof::borrow_mut(&mut grand.id, KEY);
        parent.value = v2;
        let child: &mut A = dof::borrow_mut(&mut parent.id, KEY);
        child.value = v3;
    }

    public fun remove(grand: &mut A) {
        let parent: &mut A = dof::borrow_mut(&mut grand.id, KEY);
        let A { id, value: _ } = dof::remove(&mut parent.id, KEY);
        object::delete(id);
    }

    public fun check(grand: &A, v1: u64, v2: u64, v3: Option<u64>) {
        assert!(grand.value == v1, 0);
        let parent: &A = dof::borrow(&grand.id, KEY);
        assert!(parent.value == v2, 0);
        if (option::is_some(&v3)) {
            let child: &A = dof::borrow(&parent.id, KEY);
            assert!(&child.value == option::borrow(&v3), 0);
        } else {
            assert!(!dof::exists_<u64>(&parent.id, KEY), 0);
        }
    }
}

//# run tto::M1::start --sender A

//# view-object 2,0

//# view-object 2,3

//# view-object 2,1

//# view-object 2,2

//# run tto::M1::receive --args object(2,2) receiving(2,1) --sender A

//# view-object 2,0

// The grand parent
//# view-object 2,3

//# view-object 2,1

//# view-object 2,2

//# programmable --sender A --inputs object(2,2) 1 2 3
//> tto::M1::set(Input(0), Input(1), Input(2), Input(3))

//# view-object 2,0

// The grand parent
//# view-object 2,3

//# view-object 2,1

//# view-object 2,2

//# programmable --sender A --inputs object(2,2)
//> tto::M1::remove(Input(0))

// dev-inspect with 'check' and correct values

//# programmable --sender A --inputs object(2,2)@3 0 0 vector[0] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,2)@4 1 2 vector[3] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,2)@5 1 2 vector[] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))

// dev-inspect with 'check' and _incorrect_ values

//# programmable --sender A --inputs object(2,2)@4 0 0 vector[0] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,2)@5 1 2 vector[3] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))

//# programmable --sender A --inputs object(2,2)@3 1 2 vector[] --dev-inspect
//> tto::M1::check(Input(0), Input(1), Input(2), Input(3))
